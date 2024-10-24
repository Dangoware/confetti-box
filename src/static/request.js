let progressBar;
let progressValue;
let statusNotifier;
let uploadedFilesDisplay;
let durationBox;

let uploadInProgress = false;

const TOO_LARGE_TEXT = "File is too large!";
const ERROR_TEXT = "An error occured!";

let CAPABILITIES;

async function formSubmit(form) {
    if (uploadInProgress) {
        return; // TODO: REMOVE THIS ONCE MULTIPLE CAN WORK!
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
    progressBar.value = 0;

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
    let file_duration = document.getElementById("fileDuration");

    const durationButtons = durationBox.getElementsByTagName("button");
    for (const b of durationButtons) {
        b.addEventListener("click", function (_e) {
            if (this.classList.contains("selected")) {
                return
            }
            let selected = this.parentNode.getElementsByClassName("selected");
            selected[0].classList.remove("selected");

            file_duration.value = this.dataset.durationSeconds + "s";
            this.classList.add("selected");
        });
    }
}
