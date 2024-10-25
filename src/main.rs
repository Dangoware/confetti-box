mod database;
mod strings;
mod settings;
mod endpoints;
mod utils;

use std::{fs, sync::{Arc, RwLock}};

use chrono::{DateTime, TimeDelta, Utc};
use database::{clean_loop, Database, MochiFile};
use endpoints::{lookup, server_info};
use log::info;
use rocket::{
    data::{Limits, ToByteUnit}, form::Form, fs::{FileServer, Options, TempFile}, get, http::ContentType, post, response::content::{RawCss, RawJavaScript}, routes, serde::{json::Json, Serialize}, tokio, Config, FromForm, State
};
use settings::Settings;
use strings::{parse_time_string, to_pretty_time};
use utils::{get_id, hash_file};
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
    RawCss(include_str!("../web/main.css"))
}

/// Upload handler javascript
#[get("/request.js")]
fn form_handler_js() -> RawJavaScript<&'static str> {
    RawJavaScript(include_str!("../web/request.js"))
}

#[get("/favicon.svg")]
fn favicon() -> (ContentType, &'static str) {
    (ContentType::SVG, include_str!("../web/favicon.svg"))
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
                h4 { "Upload File(s)" }
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
        hash
    );
    out_path.push(filename.clone());

    let constructed_file = MochiFile::new_with_expiry(
        raw_name,
        hash,
        out_path.clone(),
        expire_time
    );

    if !settings.overwrite
        && db.read().unwrap().files.contains_key(&constructed_file.get_key())
    {
        info!("Key already in DB, NOT ADDING");
        return Ok(Json(ClientResponse {
            status: true,
            response: "File already exists",
            name: constructed_file.name().clone(),
            url: filename,
            hash: hash.to_hex()[0..10].to_string(),
            expires: Some(constructed_file.get_expiry()),
            ..Default::default()
        }))
    }

    // Move it to the new proper place
    std::fs::rename(temp_filename, out_path)?;

    db.write().unwrap().files.insert(constructed_file.get_key(), constructed_file.clone());
    db.write().unwrap().save();

    Ok(Json(ClientResponse {
        status: true,
        name: constructed_file.name().clone(),
        url: filename,
        hash: hash.to_hex()[0..10].to_string(),
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

    #[serde(skip_serializing_if = "str::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub url: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,
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
