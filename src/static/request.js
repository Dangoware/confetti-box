let progressBar = null;
let progressValue = null;
let statusNotifier = null;

let uploadInProgress = false;
let uploadedFilesDisplay = null;

async function formSubmit(form) {
    let url = "/upload";
    let request = new XMLHttpRequest();
    request.open('POST', url, true);

    request.addEventListener('load', uploadComplete, false);
    request.addEventListener('error', networkErrorHandler, false);
    request.upload.addEventListener('progress', uploadProgress, false);

    uploadInProgress = true;
    // Create and send FormData
    try {
        request.send(new FormData(form));
    } catch (e) {
        console.log(e);
    }

    // Reset the form data since we've successfully submitted it
    form.reset();
}

function networkErrorHandler(_err) {
    uploadInProgress = false;
    console.log("An error occured while uploading");
    progressValue.textContent = "A network error occured!";
}

function uploadComplete(response) {
    let target = response.target;

    console.log(target);
    if (target.status === 200) {
        const response = JSON.parse(target.responseText);

        console.log(response);
        if (response.status) {
            progressValue.textContent = "Success";
            addToList(response.name, response.url);
        }
    } else if (target.status === 413) {
        progressValue.textContent = "File too large!";
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

function attachFormSubmitEvent(formId) {
    if (uploadInProgress) {
        return;
    }

    document.getElementById(formId).addEventListener("submit", formSubmit);
}

document.addEventListener("DOMContentLoaded", function(_event){
    attachFormSubmitEvent("uploadForm");
    progressBar = document.getElementById("uploadProgress");
    progressValue = document.getElementById("uploadProgressValue");
    statusNotifier = document.getElementById("uploadStatus");
    uploadedFilesDisplay = document.getElementById("uploadedFilesDisplay");
})
