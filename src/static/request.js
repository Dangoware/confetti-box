let statusNotifier;
let uploadedFilesDisplay;
let durationBox;

let uploadInProgress = false;

const TOO_LARGE_TEXT = "File is too large!";
const ERROR_TEXT = "An error occured!";

async function formSubmit(form) {
    if (uploadInProgress) {
        return; // TODO: REMOVE THIS ONCE MULTIPLE CAN WORK!
    }

    // Get file size and don't upload if it's too large
    let file_upload = document.getElementById("fileInput");
    let file = file_upload.files[0];
    if (file.size > file_upload.dataset.maxFilesize) {
        progressValue.textContent = TOO_LARGE_TEXT;
        console.error(
            "Provided file is too large", file.size, "bytes; max",
            CAPABILITIES.max_filesize, "bytes"
        );
        return;
    }

    let [progressBar, progressText] = addNewToList(file.name);

    let url = "/upload";
    let request = new XMLHttpRequest();
    request.open('POST', url, true);

    // Set up event listeners
    request.upload.addEventListener('progress', (p) => {uploadProgress(p, progressBar, progressText)}, false);
    request.addEventListener('load', (c) => {uploadComplete(c, progressBar, progressText)}, false);
    request.addEventListener('error', networkErrorHandler, false);

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

function uploadComplete(response, _progressBar, progressText) {
    let target = response.target;

    if (target.status === 200) {
        const response = JSON.parse(target.responseText);

        if (response.status) {
            progressText.textContent = "Success";
        } else {
            console.error("Error uploading", response)
            progressText.textContent = response.response;
        }
    } else if (target.status === 413) {
        progressText.textContent = TOO_LARGE_TEXT;
    } else {
        progressText.textContent = ERROR_TEXT;
    }

    uploadInProgress = false;
}

function addNewToList(origFileName) {
    const linkRow = uploadedFilesDisplay.appendChild(document.createElement("div"));
    const fileName = linkRow.appendChild(document.createElement("p"));
    const progressBar = linkRow.appendChild(document.createElement("progress"));
    const progressTxt = linkRow.appendChild(document.createElement("p"));

    fileName.textContent = origFileName;
    progressBar.max="100";
    progressBar.value="0";

    return [progressBar, progressTxt];
}

function uploadProgress(progress, progressBar, progressText) {
    console.log(progress);
    if (progress.lengthComputable) {
        const progressPercent = Math.floor((progress.loaded / progress.total) * 100);
        progressBar.value = progressPercent;
        console.log(progressBar.value);
        progressText.textContent = progressPercent + "%";
    }
}

// This is the entrypoint for everything basically
document.addEventListener("DOMContentLoaded", function(_event){
    document.getElementById("uploadForm").addEventListener("submit", formSubmit);
    statusNotifier = document.getElementById("uploadStatus");
    uploadedFilesDisplay = document.getElementById("uploadedFilesDisplay");
    durationBox = document.getElementById("durationBox");

    getServerCapabilities();
});

function toPrettyTime(seconds) {
    var days    = Math.floor(seconds / 86400);
    var hour    = Math.floor((seconds - (days * 86400)) / 3600);
    var mins    = Math.floor((seconds - (hour * 3600) - (days * 86400)) / 60);
    var secs    = seconds - (hour * 3600) - (mins * 60) - (days * 86400);

    if(days == 0) {days = "";} else if(days == 1) {days += "<br>day"} else {days += "<br>days"}
    if(hour == 0) {hour = "";} else if(hour == 1) {hour += "<br>hour"} else {hour += "<br>hours"}
    if(mins == 0) {mins = "";} else if(mins == 1) {mins += "<br>minute"} else {mins += "<br>minutes"}
    if(secs == 0) {secs = "";} else if(secs == 1) {secs += "<br>second"} else {secs += "<br>seconds"}

    return (days + " " + hour + " " + mins + " " + secs).trim();
}

async function getServerCapabilities() {
    const durationButtons = durationBox.getElementsByTagName("button");
    for (const b of durationButtons) {
        b.addEventListener("click", function (_e) {
            if (this.classList.contains("selected")) {
                return
            }
            let selected = this.parentNode.getElementsByClassName("selected");
            selected[0].classList.remove("selected");
            this.classList.add("selected");
        });
    }
}
