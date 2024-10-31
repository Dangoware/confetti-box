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

    await sendFile(files, duration, maxSize);
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

    await sendFile(files, duration, maxSize);
}

async function sendFile(files, duration, maxSize) {
    for (const file of files) {
        if (file.size > maxSize) {
            console.error("Provided file is too large", file.size, "bytes; max", maxSize, "bytes");
            continue;
        } else if (file.size == 0) {
            console.error("Provided file has 0 bytes");
            continue;
        }

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
        }

        // Upload the file in `chunk_size` chunks
        for (let start = 0; start < file.size; start += chunkedResponse.chunk_size) {
            const chunk = file.slice(start, start + chunkedResponse.chunk_size)

            await fetch("/upload/chunked?uuid=" + chunkedResponse.uuid, { method: 'post', body: chunk }).then(res => res.text())
        }

        console.log(await fetch("/upload/chunked?uuid=" + chunkedResponse.uuid + "&finish", { method: 'post' }).then(res => res.json()))
    }
}

function networkErrorHandler(err, progressBar, progressText, linkRow) {
    makeErrored(progressBar, progressText, linkRow, "A network error occured");
    console.error("A network error occured while uploading", err);
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
