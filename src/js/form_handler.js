function formSubmit(form) {
    let url = "/upload";
    let request = new XMLHttpRequest();
    request.open('POST', url, true);
    request.onload = function() { // request successful
        // we can use server response to our request now
        console.log(request.responseText);
    };

    request.onerror = function() {
        // request failed
    };

    // Create and send FormData
    request.send(new FormData(form));

    // Reset the form data since we've successfully submitted it
    form.reset();
}

// and you can attach form submit event like this for example
function attachFormSubmitEvent(formId){
    document.getElementById(formId).addEventListener("submit", formSubmit);
}

document.addEventListener("DOMContentLoaded", function(event){
    attachFormSubmitEvent("uploadForm");
})
