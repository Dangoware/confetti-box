/*jshint esversion: 11 */

const TOO_LARGE_TEXT = "Too large!";
const ZERO_TEXT = "File is blank!";
const ERROR_TEXT = "Error!";

async function formSubmit() {
    const form = document.getElementById("uploadForm");
    const files = form.elements.fileUpload.files;
    const duration = form.elements.duration.value;
    const maxSize = form.elements.fileUpload.dataset.maxFilesize;

    await sendFiles(files, duration, maxSize);

    // Reset the form file data since we've successfully submitted it
    form.elements.fileUpload.value = "";
}

async function dragDropSubmit(evt) {
    const form = document.getElementById("uploadForm");
    const duration = form.elements.duration.value;
    const maxSize = form.elements.fileUpload.dataset.maxFilesize;

    evt.preventDefault();

    const files = [];
    if (evt.dataTransfer.items) {
        // Use DataTransferItemList interface to access the file(s)
        [...evt.dataTransfer.items].forEach((item, _) => {
            // If dropped items aren't files, reject them
            if (item.kind === "file") {
                files.push(item.getAsFile());
            }
        });
    } else {
        // Use DataTransfer interface to access the file(s)
        [...evt.dataTransfer.files].forEach((file, _) => {
            files.push(file.name);
        });
    }

    await sendFiles(files, duration, maxSize);
}

async function pasteSubmit(evt) {
    const form = document.getElementById("uploadForm");
    const duration = form.elements.duration.value;
    const maxSize = form.elements.fileUpload.dataset.maxFilesize;

    const files = [];
    const len = evt.clipboardData.files.length;
    for (let i = 0; i < len; i++) {
        const file = evt.clipboardData.files[i];
        files.push(file);
    }

    await sendFiles(files, duration, maxSize);
}

async function sendFiles(files, duration, maxSize) {
    const inProgressUploads = new Set();
    const concurrencyLimit = 10;

    // Create a reference for the Wake Lock.
    let wakeLock = null;

    // create an async function to request a wake lock
    try {
        wakeLock = await navigator.wakeLock.request("screen");
    } catch (err) {
        console.warn("Failed to set wake-lock!");
    }


    let start = performance.now();
    for (const file of files) {
        console.log("Started upload for", file.name);

        // Start the upload and add it to the set of in-progress uploads
        let uploadPromise;
        if ('WebSocket' in window && window.WebSocket.CLOSING === 2) {
            console.log("Uploading file using Websockets");
            uploadPromise = uploadFileWebsocket(file, duration, maxSize);
        } else {
            console.log("Uploading file using Chunks");
            uploadPromise = uploadFileChunked(file, duration, maxSize);
        }
        inProgressUploads.add(uploadPromise);

        // Once an upload finishes, remove it from the set
        uploadPromise.finally(() => inProgressUploads.delete(uploadPromise));

        // If we reached the concurrency limit, wait for one of the uploads to complete
        if (inProgressUploads.size >= concurrencyLimit) {
            await Promise.race(inProgressUploads);
        }
    }

    // Wait for any remaining uploads to complete
    await Promise.allSettled(inProgressUploads);
    let end = performance.now();
    console.log(end - start);

    wakeLock.release().then(() => {
        wakeLock = null;
    });
}

async function uploadFileChunked(file, duration, maxSize) {
    const [linkRow, progressBar, progressText] = await addNewToList(file.name);
    if (file.size > maxSize) {
        console.error("Provided file is too large", file.size, "bytes; max", maxSize, "bytes");
        makeErrored(progressBar, progressText, linkRow, TOO_LARGE_TEXT);
        return;
    } else if (file.size == 0) {
        console.error("Provided file has 0 bytes");
        makeErrored(progressBar, progressText, linkRow, ZERO_TEXT);
        return;
    }

    // Get preliminary upload information
    let chunkedResponse;
    try {
        const response = await fetch("/upload/chunked", {
            method: "POST",
            body: JSON.stringify({
                "name": file.name,
                "size": file.size,
                "expire_duration": parseInt(duration),
            }),
        });
        if (!response.ok) {
            throw new Error(`Response status: ${response.status}`);
        }
        chunkedResponse = await response.json();
    } catch (error) {
        console.error(error);
        makeErrored(progressBar, progressText, linkRow, ERROR_TEXT);
    }

    // Upload the file in `chunk_size` chunks
    const chunkUploads = new Set();
    const progressValues = [];
    const concurrencyLimit = 5;
    for (let chunk_num = 0; chunk_num < Math.floor(file.size / chunkedResponse.chunk_size) + 1; chunk_num ++) {
        const offset = Math.floor(chunk_num * chunkedResponse.chunk_size);
        const chunk = file.slice(offset, offset + chunkedResponse.chunk_size);
        const url = "/upload/chunked/" + chunkedResponse.uuid + "?chunk=" + chunk_num;
        const ID = progressValues.push(0);

        let upload = new Promise(function (resolve, reject) {
            let request = new XMLHttpRequest();
            request.open("POST", url, true);
            request.upload.addEventListener('progress',
                (p) => {uploadProgress(p, progressBar, progressText, progressValues, file.size, ID);}, true
            );

            request.onload = (e) => {
                if (e.target.status >= 200 && e.target.status < 300) {
                    resolve(request.response);
                } else {
                    reject({status: e.target.status, statusText: request.statusText});
                }
            };
            request.onerror = (e) => {
                reject({status: e.target.status, statusText: request.statusText})
            };
            request.send(chunk);
        });

        chunkUploads.add(upload);
        upload.finally(() => chunkUploads.delete(upload));
        if (chunkUploads.size >= concurrencyLimit) {
            await Promise.race(chunkUploads);
        }
    }
    await Promise.allSettled(chunkUploads);

    // Finish the request and update the progress box
    const result = await fetch("/upload/chunked/" + chunkedResponse.uuid + "?finish");
    let responseJson = null;
    if (result.status == 200) {
        responseJson = await result.json()
    }
    uploadComplete(responseJson, result.status, progressBar, progressText, linkRow);
}

async function uploadFileWebsocket(file, duration, maxSize) {

    const [linkRow, progressBar, progressText] = await addNewToList(file.name);
    if (file.size > maxSize) {
        console.error("Provided file is too large", file.size, "bytes; max", maxSize, "bytes");
        makeErrored(progressBar, progressText, linkRow, TOO_LARGE_TEXT);
        return;
    } else if (file.size == 0) {
        console.error("Provided file has 0 bytes");
        makeErrored(progressBar, progressText, linkRow, ZERO_TEXT);
        return;
    }

    // Open the websocket connection
    let loc = window.location, new_uri;
    if (loc.protocol === "https:") {
        new_uri = "wss:";
    } else {
        new_uri = "ws:";
    }
    new_uri += "//" + loc.host;
    new_uri += loc.pathname + "/upload/websocket?name=" + file.name +"&size=" + file.size + "&duration=" + parseInt(duration);
    const socket = new WebSocket(new_uri);

    const chunkSize = 10_000_000;
    socket.addEventListener("open", (_event) => {
        for (let chunk_num = 0; chunk_num < Math.floor(file.size / chunkSize) + 1; chunk_num ++) {
            const offset = Math.floor(chunk_num * chunkSize);
            const chunk = file.slice(offset, offset + chunkSize);

            socket.send(chunk);
        }

        socket.send("");
    });

    return new Promise(function(resolve, reject) {
        socket.addEventListener("message", (event) => {
            const response = JSON.parse(event.data);
            if (response.mmid == null) {
                const progress = parseInt(response);
                uploadProgressWebsocket(progress, progressBar, progressText, file.size);
            } else {
                // It's so over
                socket.close();

                uploadComplete(response, 200, progressBar, progressText, linkRow);
                resolve();
            }
        });
    });
}

async function addNewToList(origFileName) {
    const uploadedFilesDisplay = document.getElementById("uploadedFilesDisplay");
    const linkRow = uploadedFilesDisplay.appendChild(document.createElement("div"));
    const fileName = linkRow.appendChild(document.createElement("p"));
    const progressBar = linkRow.appendChild(document.createElement("progress"));
    const progressTxt = linkRow.appendChild(document.createElement("p"));

    fileName.textContent = origFileName;
    fileName.classList.add("file_name");
    progressTxt.classList.add("status");
    progressTxt.textContent = "⏳";
    progressBar.max="100";

    return [linkRow, progressBar, progressTxt];
}

function uploadProgress(progress, progressBar, progressText, progressValues, fileSize, ID) {
    if (progress.lengthComputable) {
        progressValues[ID] = progress.loaded;
        const progressTotal = progressValues.reduce((a, b) => a + b, 0);

        const progressPercent = Math.floor((progressTotal / fileSize) * 100);
        if (progressPercent == 100) {
            progressBar.removeAttribute("value");
            progressText.textContent = "⏳";
        } else {
            progressBar.value = progressTotal;
            progressBar.max = fileSize;
            progressText.textContent = progressPercent + "%";
        }
    }
}

function uploadProgressWebsocket(bytesFinished, progressBar, progressText, fileSize) {
    const progressPercent = Math.floor((bytesFinished / fileSize) * 100);
    if (progressPercent == 100) {
        progressBar.removeAttribute("value");
        progressText.textContent = "⏳";
    } else {
        progressBar.value = bytesFinished;
        progressBar.max = fileSize;
        progressText.textContent = progressPercent + "%";
    }
}

async function uploadComplete(responseJson, status, progressBar, progressText, linkRow) {
    if (status === 200) {
        console.log("Successfully uploaded file", responseJson);
        makeFinished(progressBar, progressText, linkRow, responseJson);
    } else if (status === 413) {
        makeErrored(progressBar, progressText, linkRow, TOO_LARGE_TEXT);
    } else {
        makeErrored(progressBar, progressText, linkRow, ERROR_TEXT);
    }
}

function makeErrored(progressBar, progressText, linkRow, errorMessage) {
    progressText.textContent = errorMessage;
    progressBar.style.display = "none";
    linkRow.classList.add("upload_failed");
}

function makeFinished(progressBar, progressText, linkRow, response) {
    progressText.textContent = "";
    const link = progressText.appendChild(document.createElement("a"));
    link.textContent = response.mmid;
    link.href = "/f/" + response.mmid;
    link.target = "_blank";

    let button = linkRow.appendChild(document.createElement("button"));
    button.textContent = "📝";
    let buttonTimeout = null;
    button.addEventListener('click', function(_e) {
        const mmid = response.mmid;
        if (buttonTimeout) {
            clearTimeout(buttonTimeout);
        }
        navigator.clipboard.writeText(
                window.location.protocol + "//" + window.location.host + "/f/" + mmid
        );
        button.textContent = "✅";
        buttonTimeout = setTimeout(function() {
            button.textContent = "📝";
        }, 750);
    });

    progressBar.style.display = "none";
    linkRow.classList.add("upload_done");
}

async function initEverything() {
    const durationBox = document.getElementById("durationBox");
    const durationButtons = durationBox.getElementsByTagName("button");
    for (const b of durationButtons) {
        b.addEventListener("click", function (_e) {
            if (this.classList.contains("selected")) {
                return;
            }
            document.getElementById("uploadForm").elements.duration.value = this.dataset.durationSeconds;
            let selected = this.parentNode.getElementsByClassName("selected");
            selected[0].classList.remove("selected");
            this.classList.add("selected");
        });
    }
}

// This is the entrypoint for everything basically
document.addEventListener("DOMContentLoaded", function(_event) {
    // Respond to form submissions
    const form = document.getElementById("uploadForm");
    form.addEventListener("submit", formSubmit);

    // Respond to file paste events
    window.addEventListener("paste", (event) => {
        pasteSubmit(event)
    });

    // Respond to drag and drop stuff
    let fileButton = document.getElementById("fileButton");
    document.addEventListener("drop", (e) => {e.preventDefault();}, false);
    document.addEventListener("dragover", (e) => {e.preventDefault()}, false);
    fileButton.addEventListener("dragover", (e) => {e.preventDefault();}, false);
    fileButton.addEventListener("drop", dragDropSubmit, false);

    initEverything();
});
