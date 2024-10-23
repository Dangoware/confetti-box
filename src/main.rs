mod database;
mod time_string;
mod settings;

use std::{path::{Path, PathBuf}, sync::{Arc, RwLock}, time::Duration};
use blake3::Hash;
use chrono::{DateTime, TimeDelta, Utc};
use database::{clean_loop, Database, MochiFile};
use log::info;
use rocket::{
    data::{Limits, ToByteUnit}, form::Form, fs::{FileServer, Options, TempFile}, get, post, response::content::{RawCss, RawJavaScript}, routes, serde::{json::Json, Serialize}, tokio::{self, fs::File, io::AsyncReadExt}, Config, FromForm, State
};
use settings::Settings;
use time_string::parse_time_string;
use uuid::Uuid;
use maud::{html, Markup, DOCTYPE};

fn head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="UTF-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        title { (page_title) }
        // Javascript stuff for client side handling
        script src="request.js" { }
    }
}

/// Stylesheet
#[get("/main.css")]
fn stylesheet() -> RawCss<&'static str> {
    RawCss(include_str!("static/main.css"))
}

/// Upload handler javascript
#[get("/request.js")]
fn form_handler_js() -> RawJavaScript<&'static str> {
    RawJavaScript(include_str!("static/request.js"))
}

#[get("/")]
fn home() -> Markup {
    html! {
        (head("Mochi"))
        body {
            main {
                section class="centered" {
                    form id="uploadForm" {
                        label for="fileUpload" class="file-upload" onclick="document.getElementById('fileInput').click()" {
                            "Upload File"
                        }
                        input id="fileInput" type="file" name="fileUpload" onchange="formSubmit(this.parentNode)" style="display:none;";
                        br;
                        input type="text" name="duration" minlength="2" maxlength="4";
                    }
                    div class="progress-box" {
                        progress id="uploadProgress" value="0" max="100" {}
                        p id="uploadProgressValue" class="progress-value" { "0%" }
                    }
                }

                section class="centered" id="uploadedFilesDisplay" {
                    h2 class="sep center" { "Uploaded Files" }
                }
            }
        }
    }
}

#[derive(FromForm)]
struct Upload<'r> {
    #[field(name = "fileUpload")]
    file: TempFile<'r>,

    #[field(name = "duration")]
    expire_time: String,
}

/// Handle a file upload and store it
#[post("/upload", data = "<file_data>")]
async fn handle_upload(
    mut file_data: Form<Upload<'_>>,
    db: &State<Arc<RwLock<Database>>>
) -> Result<Json<ClientResponse>, std::io::Error> {
    let mut out_path = PathBuf::from("files/");

    let expire_time = if let Ok(t) = parse_time_string(&file_data.expire_time) {
        if t < TimeDelta::days(365) {
            t
        } else {
            TimeDelta::days(365)
        }
    } else {
        return Ok(Json(ClientResponse {
            status: false,
            response: "Invalid duration",
            ..Default::default()
        }))
    };

    // Get temp path and hash it
    let temp_filename = "temp_files/".to_owned() + &Uuid::new_v4().to_string();
    file_data.file.persist_to(&temp_filename).await?;
    let hash = hash_file(&temp_filename).await?;

    // TODO: Properly sanitize this...
    let raw_name = file_data.file.raw_name().unwrap().dangerous_unsafe_unsanitized_raw().as_str();
    let filename = get_id(
        raw_name,
        hash.0
    );
    out_path.push(filename.clone());

    let constructed_file = MochiFile::new_with_expiry(
        raw_name,
        hash.1,
        hash.0,
        out_path.clone(),
        expire_time
    );

    // Move it to the new proper place
    std::fs::rename(temp_filename, out_path)?;

    db.write().unwrap().files.insert(constructed_file.get_key(), constructed_file.clone());
    db.write().unwrap().save();

    Ok(Json(ClientResponse {
        status: true,
        name: Some(constructed_file.name().clone()),
        url: Some("files/".to_string() + &filename),
        expires: Some(constructed_file.get_expiry()),
        ..Default::default()
    }))
}

/// A response to the client from the server
#[derive(Serialize, Default, Debug)]
#[serde(crate = "rocket::serde")]
struct ClientResponse {
    /// Success or failure
    pub status: bool,

    pub response: &'static str,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,
}

/// Get a filename based on the file's hashed name
fn get_id(name: &str, hash: Hash) -> String {
    hash.to_hex()[0..10].to_string() + "_" + name
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

/// An endpoint to obtain information about the server's capabilities
#[get("/info")]
fn server_info(settings: &State<Settings>) -> Json<ServerInfo> {
    Json(ServerInfo {
        max_filesize: settings.max_filesize,
        max_duration: settings.max_duration,
    })
}

#[derive(Serialize, Debug)]
#[serde(crate = "rocket::serde")]
struct ServerInfo {
    max_filesize: u64,
    max_duration: u32,
}

#[rocket::main]
async fn main() {
    // Get or create config file
    let config = Settings::open(&"./settings.toml")
        .expect("Could not open settings file");

    // Set rocket configuration settings
    let rocket_config = Config {
        address: config.server.address.parse().expect("IP address invalid"),
        port: config.server.port,
        temp_dir: config.temp_dir.clone().into(),
        limits: Limits::default()
            .limit("data-form", config.max_filesize.bytes())
            .limit("file", config.max_filesize.bytes()),
        ..Default::default()
    };

    let database = Arc::new(RwLock::new(Database::open(&"database.mochi")));
    let local_db = database.clone();

    // Start monitoring thread
    let (shutdown, rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn({
        let cleaner_db = database.clone();
        async move { clean_loop(cleaner_db, rx, Duration::from_secs(120)).await }
    });

    let rocket = rocket::build()
        .mount(
            config.root_path.clone() + "/",
            routes![home, handle_upload, form_handler_js, stylesheet, server_info]
        )
        .mount(
            config.root_path.clone() + "/files",
            FileServer::new("files/", Options::Missing | Options::NormalizeDirs)
        )
        .manage(database)
        .manage(config)
        .configure(rocket_config)
        .launch()
        .await;

    // Ensure the server gracefully shuts down
    rocket.expect("Server failed to shutdown gracefully");

    info!("Stopping database cleaning thread");
    shutdown.send(()).await.expect("Failed to stop cleaner thread");

    info!("Saving database on shutdown...");
    local_db.write().unwrap().save();
}
