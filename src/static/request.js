let progressBar;
let progressValue;
let statusNotifier;
let uploadedFilesDisplay;

let uploadInProgress = false;

const TOO_LARGE_TEXT = "File is too large!";
const ERROR_TEXT = "An error occured!";

let CAPABILITIES;
async function getServerCapabilities() {
    CAPABILITIES = await fetch("info").then((response) => response.json());
    console.log(CAPABILITIES);
}
getServerCapabilities();

async function formSubmit(form) {
    if (uploadInProgress) {
        return;
    }

    // Get file size and don't upload if it's too large
    let file_upload = document.getElementById("fileInput");
    let file = file_upload.files[0];
    if (file.size > CAPABILITIES.max_filesize) {
        progressValue.textContent = TOO_LARGE_TEXT;
        console.error(
            "Provided file is too large", file.size, "bytes; max",
            CAPABILITIES.max_filesize, "bytes"
        );
        return;
    }

    let url = "/upload";
    let request = new XMLHttpRequest();
    request.open('POST', url, true);

    // Set up the listeners
    request.addEventListener('load', uploadComplete, false);
    request.addEventListener('error', networkErrorHandler, false);
    request.upload.addEventListener('progress', uploadProgress, false);

    uploadInProgress = true;
    // Create and send FormData
    try {
        request.send(new FormData(form));
    } catch (e) {
        console.error("An error occured while uploading", e);
    }

    // Reset the form file data since we've successfully submitted it
    form.elements["fileUpload"].value = "";
}

function networkErrorHandler(_err) {
    uploadInProgress = false;
    console.error("A network error occured while uploading");
    progressValue.textContent = "A network error occured!";
}

function uploadComplete(response) {
    let target = response.target;

    if (target.status === 200) {
        const response = JSON.parse(target.responseText);

        if (response.status) {
            progressValue.textContent = "Success";
            addToList(response.name, response.url);
        } else {
            console.error("Error uploading", response)
            progressValue.textContent = response.response;
        }
    } else if (target.status === 413) {
        progressValue.textContent = TOO_LARGE_TEXT;
    } else {
        progressValue.textContent = ERROR_TEXT;
    }

    uploadInProgress = false;
}

function addToList(filename, link) {
    const link_row = uploadedFilesDisplay.appendChild(document.createElement("p"));
    const new_link = link_row.appendChild(document.createElement("a"));

    new_link.href = link;
    new_link.textContent = filename;
}

function uploadProgress(progress) {
    if (progress.lengthComputable) {
        const progressPercent = Math.floor((progress.loaded / progress.total) * 100);
        progressBar.value = progressPercent;
        progressValue.textContent = progressPercent + "%";
    }
}

document.addEventListener("DOMContentLoaded", function(_event){
    document.getElementById("uploadForm").addEventListener("submit", formSubmit);
    progressBar = document.getElementById("uploadProgress");
    progressValue = document.getElementById("uploadProgressValue");
    statusNotifier = document.getElementById("uploadStatus");
    uploadedFilesDisplay = document.getElementById("uploadedFilesDisplay");
});
