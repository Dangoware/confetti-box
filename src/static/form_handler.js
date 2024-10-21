let progressBar = null;
let progressValue = null;

function formSubmit(form) {
    let url = "/upload";
    let request = new XMLHttpRequest();
    request.open('POST', url, true);
    request.onload = function() { // request successful
        console.log(request.responseText);
    };

    request.upload.onprogress = uploadProgress;

    request.onerror = function() {
        console.log(request.responseText);
    };

    // Create and send FormData
    request.send(new FormData(form));

    // Reset the form data since we've successfully submitted it
    form.reset();
}

function uploadProgress(progress) {
    if (progress.lengthComputable) {
        const progressPercent = Math.floor((progress.loaded / progress.total) * 100);
        progressBar.value = progressPercent;
        progressValue.textContent = progressPercent + "%";
    }
}

function attachFormSubmitEvent(formId) {
    document.getElementById(formId).addEventListener("submit", formSubmit);
}

document.addEventListener("DOMContentLoaded", function(_event){
    attachFormSubmitEvent("uploadForm");
    progressBar = document.getElementById("uploadProgress");
    progressValue = document.getElementById("uploadProgressValue");
})
