use std::path::PathBuf;

use maud::{html, Markup, DOCTYPE, PreEscaped};

use rocket::{
    form::Form, fs::{FileServer, Options, TempFile}, get, post, routes, FromForm
};
use uuid::Uuid;

const FORM_HANDLER_JS: &str = include_str!("js/form_handler.js");

fn head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        title { (page_title) }
        script { (PreEscaped(FORM_HANDLER_JS)) }
    }
}

#[get("/")]
fn home() -> Markup {
    html! {
        (head("Mochi"))
        body {
            h1 { "File Hosting!" }
            h2 { "Everything will be deleted in like 24 hours or something idk" }
            form id="uploadForm" {
                input type="file" name="fileUpload" onchange="formSubmit(this.parentNode)" {
                    "Select File (Or drag here)"
                }
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
    out_path.push(get_filename());
    file_data.file.persist_to(out_path).await?;

    Ok(())
}

/// Get a random filename for use as the uploaded file's name
fn get_filename() -> String {
    let uuid = Uuid::new_v4().to_string();
    uuid
}

#[rocket::launch]
fn launch() -> _ {
    rocket::build()
        .mount("/", routes![home, handle_upload])
        .mount("/files", FileServer::new("files/", Options::Missing | Options::NormalizeDirs))
}
