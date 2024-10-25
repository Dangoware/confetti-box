let statusNotifier;
let uploadedFilesDisplay;
let durationBox;

const TOO_LARGE_TEXT = "Too large!";
const ERROR_TEXT = "Error!";

async function formSubmit(form) {
    // Get file size and don't upload if it's too large
    let file_upload = document.getElementById("fileInput");

    for (const file of file_upload.files) {
        let [linkRow, progressBar, progressText] = addNewToList(file.name);
        if (file.size > file_upload.dataset.maxFilesize) {
            makeErrored(progressBar, progressText, linkRow, TOO_LARGE_TEXT);
            console.error(
                "Provided file is too large", file.size, "bytes; max",
                file_upload.dataset.maxFilesize, "bytes"
            );
            continue
        }

        let request = new XMLHttpRequest();
        request.open('POST', "./upload", true);

        // Set up event listeners
        request.upload.addEventListener('progress',
            (p) => {uploadProgress(p, progressBar, progressText, linkRow)}, false);
        request.addEventListener('load',
            (c) => {uploadComplete(c, progressBar, progressText, linkRow)}, false);
        request.addEventListener('error',
            (e) => {networkErrorHandler(e, progressBar, progressText, linkRow)}, false);

        // Create and send FormData
        try {
            const formData = new FormData();
            formData.append("fileUpload", file);
            formData.append("duration", form.elements["duration"].value);
            request.send(formData);
        } catch (e) {
            makeErrored(progressBar, progressText, linkRow, ERROR_TEXT);
            console.error("An error occured while uploading", e);
        }
    }

    // Reset the form file data since we've successfully submitted it
    form.elements["fileUpload"].value = "";
}

function makeErrored(progressBar, progressText, linkRow, errorMessage) {
    progressText.textContent = errorMessage;
    progressBar.style.display = "none";
    linkRow.style.background = "#ffb2ae";
}

function makeFinished(progressBar, progressText, linkRow, linkAddress, hash) {
    progressText.textContent = "";
    const link = progressText.appendChild(document.createElement("a"));
    link.textContent = hash;
    link.href = "/files/" + linkAddress;
    link.target = "_blank";

    let button = linkRow.appendChild(document.createElement("button"));
    button.textContent = "📝";
    let buttonTimeout = null;
    button.addEventListener('click', function(_e) {
        if (buttonTimeout) {
            clearTimeout(buttonTimeout)
        }
        navigator.clipboard.writeText(
            encodeURI(window.location.protocol + "//" + window.location.host + "/files/" + linkAddress)
        )
        button.textContent = "✅";
        buttonTimeout = setTimeout(function() {
            button.textContent = "📝";
        }, 750);
    })

    progressBar.style.display = "none";
    linkRow.style.background = "#a4ffbb";
}

function networkErrorHandler(err, progressBar, progressText, linkRow) {
    makeErrored(progressBar, progressText, linkRow, "A network error occured");
    console.error("A network error occured while uploading", err);
}

function uploadComplete(response, progressBar, progressText, linkRow) {
    let target = response.target;

    if (target.status === 200) {
        const response = JSON.parse(target.responseText);

        if (response.status) {
            makeFinished(progressBar, progressText, linkRow, response.url, response.hash);
        } else {
            console.error("Error uploading", response)
            makeErrored(progressBar, progressText, linkRow, response.response);
        }
    } else if (target.status === 413) {
        makeErrored(progressBar, progressText, linkRow, TOO_LARGE_TEXT);
    } else {
        makeErrored(progressBar, progressText, linkRow, ERROR_TEXT);
    }
}

function addNewToList(origFileName) {
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

function uploadProgress(progress, progressBar, progressText, linkRow) {
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

// This is the entrypoint for everything basically
document.addEventListener("DOMContentLoaded", function(_event){
    document.getElementById("uploadForm").addEventListener("submit", formSubmit);
    statusNotifier = document.getElementById("uploadStatus");
    uploadedFilesDisplay = document.getElementById("uploadedFilesDisplay");
    durationBox = document.getElementById("durationBox");

    initEverything();
});

async function initEverything() {
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
