/*jshint esversion: 11 */

const TOO_LARGE_TEXT = "Too large!";
const ZERO_TEXT = "File is blank!";
const ERROR_TEXT = "Error!";

async function formSubmit() {
    const form = document.getElementById("uploadForm");
    const files = form.elements.fileUpload.files;
    const duration = form.elements.duration.value;
    const maxSize = form.elements.fileUpload.dataset.maxFilesize;

    await sendFile(files, duration, maxSize);

    // Reset the form file data since we've successfully submitted it
    form.elements.fileUpload.value = "";
}

async function dragDropSubmit(evt) {
    const form = document.getElementById("uploadForm");
    const duration = form.elements.duration.value;

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

    await sendFile(files, duration);
}

async function pasteSubmit(evt) {
    const form = document.getElementById("uploadForm");
    const duration = form.elements.duration.value;

    const files = [];
    const len = evt.clipboardData.files.length;
    for (let i = 0; i < len; i++) {
        const file = evt.clipboardData.files[i];
        files.push(file);
    }

    await sendFile(files, duration);
}

async function sendFile(files, duration, maxSize) {
    for (const file of files) {
        const [linkRow, progressBar, progressText] = addNewToList(file.name);
        if (file.size > maxSize) {
            makeErrored(progressBar, progressText, linkRow, TOO_LARGE_TEXT);
            console.error("Provided file is too large", file.size, "bytes; max", maxSize, "bytes");
            continue;
        } else if (file.size == 0) {
            makeErrored(progressBar, progressText, linkRow, ZERO_TEXT);
            console.error("Provided file has 0 bytes");
            continue;
        }

        const request = new XMLHttpRequest();
        request.open('POST', "./upload", true);

        // Set up event listeners
        request.upload.addEventListener('progress',
            (p) => {uploadProgress(p, progressBar, progressText, linkRow);}, false);
        request.addEventListener('load',
            (c) => {uploadComplete(c, progressBar, progressText, linkRow);}, false);
        request.addEventListener('error',
            (e) => {networkErrorHandler(e, progressBar, progressText, linkRow);}, false);

        linkRow.classList.add("upload_inprogress");

        // Create and send FormData
        try {
            const formData = new FormData();
            formData.append("duration", duration);
            formData.append("fileUpload", file);
            request.send(formData);
        } catch (e) {
            makeErrored(progressBar, progressText, linkRow, ERROR_TEXT);
            console.error("An error occured while uploading", e);
        }
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

function networkErrorHandler(err, progressBar, progressText, linkRow) {
    makeErrored(progressBar, progressText, linkRow, "A network error occured");
    console.error("A network error occured while uploading", err);
}

function uploadProgress(progress, progressBar, progressText, _linkRow) {
    if (progress.lengthComputable) {
        const progressPercent = Math.floor((progress.loaded / progress.total) * 100);
        if (progressPercent == 100) {
            progressBar.removeAttribute("value");
            progressText.textContent = "⏳";
        } else {
            progressBar.value = progressPercent;
            progressText.textContent = progressPercent + "%";
        }
    }
}

function uploadComplete(response, progressBar, progressText, linkRow) {
    let target = response.target;

    if (target.status === 200) {
        const response = JSON.parse(target.responseText);

        if (response.status) {
            console.log("Successfully uploaded file", response);
            makeFinished(progressBar, progressText, linkRow, response);
        } else {
            console.error("Error uploading", response);
            makeErrored(progressBar, progressText, linkRow, response.response);
        }
    } else if (target.status === 413) {
        makeErrored(progressBar, progressText, linkRow, TOO_LARGE_TEXT);
    } else {
        makeErrored(progressBar, progressText, linkRow, ERROR_TEXT);
    }
}

function addNewToList(origFileName) {
    const uploadedFilesDisplay = document.getElementById("uploadedFilesDisplay");
    const linkRow = uploadedFilesDisplay.appendChild(document.createElement("div"));
    const fileName = linkRow.appendChild(document.createElement("p"));
    const progressBar = linkRow.appendChild(document.createElement("progress"));
    const progressTxt = linkRow.appendChild(document.createElement("p"));

    fileName.textContent = origFileName;
    fileName.classList.add("file_name");
    progressTxt.classList.add("status");
    progressBar.max="100";
    progressBar.value="0";

    return [linkRow, progressBar, progressTxt];
}

async function initEverything() {
    const durationBox = document.getElementById("durationBox");
    const durationButtons = durationBox.getElementsByTagName("button");
    for (const b of durationButtons) {
        b.addEventListener("click", function (_e) {
            if (this.classList.contains("selected")) {
                return;
            }
            document.getElementById("uploadForm").elements.duration.value = this.dataset.durationSeconds + "s";
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
