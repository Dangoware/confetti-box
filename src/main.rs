mod database;

use std::{path::{Path, PathBuf}, sync::{Arc, RwLock}};
use blake3::Hash;
use chrono::TimeDelta;
use database::{Database, MochiFile};
use log::info;
use maud::{html, Markup, DOCTYPE, PreEscaped};
use rocket::{
    form::Form, fs::{FileServer, Options, TempFile}, get, post, routes, tokio::{fs::File, io::AsyncReadExt}, FromForm, State
};
use uuid::Uuid;

fn head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="UTF-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
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
            div class="main-wrapper" {
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
}

#[derive(FromForm)]
struct Upload<'r> {
    #[field(name = "fileUpload")]
    file: TempFile<'r>,
}

/// Handle a file upload and store it
#[post("/upload", data = "<file_data>")]
async fn handle_upload(
    mut file_data: Form<Upload<'_>>,
    db: &State<Arc<RwLock<Database>>>
) -> Result<(), std::io::Error> {
    let mut out_path = PathBuf::from("files/");

    // Get temp path and hash it
    let temp_filename = "temp_files/".to_owned() + &Uuid::new_v4().to_string();
    file_data.file.persist_to(&temp_filename).await?;
    let hash = hash_file(&temp_filename).await?;

    let filename = get_filename(
        // TODO: Properly sanitize this...
        file_data.file.raw_name().unwrap().dangerous_unsafe_unsanitized_raw().as_str(),
        hash.0
    );
    out_path.push(filename);

    let constructed_file = MochiFile::new_with_expiry(
        file_data.file.raw_name().unwrap().dangerous_unsafe_unsanitized_raw().as_str(),
        hash.1,
        hash.0,
        out_path.clone(),
        TimeDelta::hours(24)
    );

    // Move it to the new proper place
    std::fs::rename(temp_filename, out_path)?;

    db.write().unwrap().files.insert(constructed_file.get_key(), constructed_file);
    db.write().unwrap().save();

    Ok(())
}

/// Get the Blake3 hash of a file, without reading it all into memory, and also get the size
async fn hash_file<P: AsRef<Path>>(input: &P) -> Result<(Hash, usize), std::io::Error> {
    let mut file = File::open(input).await?;
    let mut buf = vec![0; 5000000];
    let mut hasher = blake3::Hasher::new();

    let mut total = 0;
    let mut bytes_read = None;
    while bytes_read != Some(0) {
        bytes_read = Some(file.read(&mut buf).await?);
        total += bytes_read.unwrap();
        hasher.update(&buf[..bytes_read.unwrap()]);
    }

    Ok((hasher.finalize(), total))
}

/// Get a random filename for use as the uploaded file's name
fn get_filename(name: &str, hash: Hash) -> String {
    hash.to_hex()[0..10].to_string() + "_" + name
}

/*
/// Handle a file upload and store it
#[post("/query", data = "<file_data>")]
async fn handle_upload(
    mut file_data: Form<Upload<'_>>,
    db: &State<Arc<RwLock<Database>>>
) -> Result<(), std::io::Error> {

    Ok(())
}
*/

#[rocket::main]
async fn main() {
    let database = Arc::new(RwLock::new(Database::open(&"database.mochi")));
    let local_db = database.clone();

    let rocket = rocket::build()
        .manage(database)
        .mount("/", routes![home, handle_upload])
        .mount("/files", FileServer::new("files/", Options::Missing | Options::NormalizeDirs))
        .launch()
        .await;

    rocket.expect("Server failed to shutdown gracefully");

    info!("Saving database on shutdown...");
    local_db.write().unwrap().save();
}
