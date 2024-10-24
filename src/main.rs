mod database;
mod strings;
mod settings;

use std::{fs, path::Path, sync::{Arc, RwLock}};
use blake3::Hash;
use chrono::{DateTime, TimeDelta, Utc};
use database::{clean_loop, Database, MochiFile};
use log::info;
use rocket::{
    config, data::{Limits, ToByteUnit}, form::Form, fs::{FileServer, Options, TempFile}, get, http::{ContentType, RawStr}, post, response::{content::{RawCss, RawJavaScript}, status::NotFound, Redirect}, routes, serde::{json::Json, Serialize}, tokio::{self, fs::File, io::AsyncReadExt}, Config, FromForm, State
};
use settings::Settings;
use strings::{parse_time_string, to_pretty_time};
use uuid::Uuid;
use maud::{html, Markup, DOCTYPE, PreEscaped};

fn head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="UTF-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        title { (page_title) }
        // Javascript stuff for client side handling
        script src="./request.js" { }
        link rel="icon" type="image/svg+xml" href="favicon.svg";
        link rel="stylesheet" href="./main.css";
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

#[get("/favicon.svg")]
fn favicon() -> (ContentType, &'static str) {
    (ContentType::SVG, include_str!("static/favicon.svg"))
}

#[get("/")]
fn home(settings: &State<Settings>) -> Markup {
    html! {
        (head("Confetti-Box"))

        center {
            h1 { "Confetti-Box 🎉" }
            h2 { "Files up to " (settings.max_filesize.bytes()) " in size are allowed!" }
            hr;
            button.main_file_upload onclick="document.getElementById('fileInput').click()" {
                h4 { "Upload File" }
                p { "Click or Drag and Drop" }
            }
            h3 { "Expire after:" }
            div id="durationBox" {
                @for d in &settings.duration.allowed {
                    button.button.{@if settings.duration.default == *d { "selected" }}
                        data-duration-seconds=(d.num_seconds())
                    {
                        (PreEscaped(to_pretty_time(d.num_seconds() as u32)))
                    }
                }
            }
            form #uploadForm {
                // It's stupid how these can't be styled so they're just hidden here...
                input id="fileInput" type="file" name="fileUpload" multiple
                    onchange="formSubmit(this.parentNode)" data-max-filesize=(settings.max_filesize) style="display:none;";
                input id="fileDuration" type="text" name="duration" minlength="2"
                    maxlength="7" value=(settings.duration.default.num_seconds().to_string() + "s") style="display:none;";
            }
            hr;

            h3 { "Uploaded Files" }
            div #uploadedFilesDisplay {

            }

            hr;
            footer {
                p {a href="https://github.com/G2-Games/confetti-box" {"Source"}}
                p {a href="https://g2games.dev/" {"My Website"}}
                p {a href="#" {"Links"}}
                p {a href="#" {"Go"}}
                p {a href="#" {"Here"}}
            }
        }
    }
}

#[derive(Debug, FromForm)]
struct Upload<'r> {
    #[field(name = "duration")]
    expire_time: String,

    #[field(name = "fileUpload")]
    file: TempFile<'r>,
}

/// Handle a file upload and store it
#[post("/upload", data = "<file_data>")]
async fn handle_upload(
    mut file_data: Form<Upload<'_>>,
    db: &State<Arc<RwLock<Database>>>,
    settings: &State<Settings>,
) -> Result<Json<ClientResponse>, std::io::Error> {
    let mut temp_dir = settings.temp_dir.clone();
    let mut out_path = settings.file_dir.clone();

    let expire_time = if let Ok(t) = parse_time_string(&file_data.expire_time) {
        if t > settings.duration.maximum {
            return Ok(Json(ClientResponse {
                status: false,
                response: "Duration larger than maximum",
                ..Default::default()
            }))
        }

        if settings.duration.restrict_to_allowed && !settings.duration.allowed.contains(&t) {
            return Ok(Json(ClientResponse {
                status: false,
                response: "Duration is disallowed",
                ..Default::default()
            }))
        }

        t
    } else {
        return Ok(Json(ClientResponse {
            status: false,
            response: "Invalid duration",
            ..Default::default()
        }))
    };

    // TODO: Properly sanitize this...
    let raw_name = &*file_data.file
        .raw_name()
        .unwrap()
        .dangerous_unsafe_unsanitized_raw()
        .as_str()
        .to_string();

    // Get temp path and hash it
    temp_dir.push(Uuid::new_v4().to_string());
    let temp_filename = temp_dir;
    file_data.file.persist_to(&temp_filename).await?;
    let hash = hash_file(&temp_filename).await?;

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

    if db.read().unwrap().files.contains_key(&constructed_file.get_key()) {
        info!("Key already in DB");
    }

    // Move it to the new proper place
    std::fs::rename(temp_filename, out_path)?;

    db.write().unwrap().files.insert(constructed_file.get_key(), constructed_file.clone());
    db.write().unwrap().save();

    Ok(Json(ClientResponse {
        status: true,
        name: constructed_file.name().clone(),
        url: "files/".to_string() + &filename,
        hash: hash.0.to_hex()[0..10].to_string(),
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

    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hash: String,
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
        max_duration: settings.duration.maximum.num_seconds() as u32,
        default_duration: settings.duration.default.num_seconds() as u32,
        allowed_durations: settings.duration.allowed.clone().into_iter().map(|t| t.num_seconds() as u32).collect(),
    })
}

#[derive(Serialize, Debug)]
#[serde(crate = "rocket::serde")]
struct ServerInfo {
    max_filesize: u64,
    max_duration: u32,
    default_duration: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    allowed_durations: Vec<u32>,
}

/// Look up the hash of a file to find it. This only returns the first
/// hit for a hash, so different filenames may not be found.
#[get("/f/<id>")]
fn lookup(
    db: &State<Arc<RwLock<Database>>>,
    id: &str
) -> Result<Redirect, NotFound<()>> {
    for file in db.read().unwrap().files.values() {
        if file.hash().to_hex()[0..10].to_string() == id {
            let filename = get_id(
                file.name(),
                *file.hash()
            );
            let filename = RawStr::new(&filename).percent_encode().to_string();
            return Ok(Redirect::to(format!("/files/{}", filename)))
        }
    }

    Err(NotFound(()))
}

#[rocket::main]
async fn main() {
    // Get or create config file
    let config = Settings::open(&"./settings.toml")
        .expect("Could not open settings file");

    if !config.temp_dir.try_exists().is_ok_and(|e| e) {
        fs::create_dir_all(config.temp_dir.clone()).expect("Failed to create temp directory");
    }

    if !config.file_dir.try_exists().is_ok_and(|e| e) {
        fs::create_dir_all(config.file_dir.clone()).expect("Failed to create file directory");
    }

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

    let database = Arc::new(RwLock::new(Database::open(&config.database_path)));
    let local_db = database.clone();

    // Start monitoring thread, cleaning the database every 2 minutes
    let (shutdown, rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn({
        let cleaner_db = database.clone();
        async move { clean_loop(cleaner_db, rx, TimeDelta::minutes(2)).await }
    });

    let rocket = rocket::build()
        .mount(
            config.server.root_path.clone() + "/",
            routes![home, handle_upload, form_handler_js, stylesheet, server_info, favicon, lookup]
        )
        .mount(
            config.server.root_path.clone() + "/files",
            FileServer::new(config.file_dir.clone(), Options::Missing | Options::NormalizeDirs)
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
