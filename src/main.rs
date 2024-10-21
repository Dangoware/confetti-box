use std::path::PathBuf;

use maud::{html, Markup, DOCTYPE, PreEscaped};

use rand::Rng;
use rocket::{
    form::Form, fs::{FileServer, Options, TempFile}, get, post, routes, FromForm
};

fn head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        title { (page_title) }
        // Javascript stuff for client side handling
        script { (PreEscaped(include_str!("static/form_handler.js"))) }
        // CSS for styling the sheets
        style { (PreEscaped(include_str!("static/main.css"))) }
    }
}

#[get("/")]
fn home() -> Markup {
    html! {
        (head("Mochi"))
        body {
            h1 { "File Hosting!" }
            form id="uploadForm" {
                label for="fileUpload" class="file-upload" onclick="document.getElementById('fileInput').click()" {
                    "Upload File"
                }
                input id="fileInput" type="file" name="fileUpload" onchange="formSubmit(this.parentNode)" style="display:none;";
            }
            div class="progress-box" {
                progress id="uploadProgress" value="0" max="100" {}
                p id="uploadProgressValue" class="progress-value" { "0%" }
            }
        }
    }
}

#[derive(FromForm)]
struct Upload<'r> {
    #[field(name = "fileUpload")]
    file: TempFile<'r>,
}

/// Handle a file upload and store it
#[post("/upload", data = "<file_data>")]
async fn handle_upload(mut file_data: Form<Upload<'_>>) -> Result<(), std::io::Error> {
    let mut out_path = PathBuf::from("files/");
    out_path.push(get_filename(file_data.file.name().unwrap()));
    file_data.file.persist_to(out_path).await?;

    Ok(())
}

/// Get a random filename for use as the uploaded file's name
fn get_filename(name: &str) -> String {
    let rand_string: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    let uuid = rand_string + "_" + name;
    uuid
}

#[rocket::launch]
fn launch() -> _ {
    rocket::build()
        .mount("/", routes![home, handle_upload])
        .mount("/files", FileServer::new("files/", Options::Missing | Options::NormalizeDirs))
}
