mod database;
mod endpoints;
mod settings;
mod strings;
mod utils;
mod pages;

use std::{
    fs,
    sync::{Arc, RwLock},
};

use chrono::{DateTime, TimeDelta, Utc};
use database::{clean_loop, Database, Mmid, MochiFile};
use endpoints::{lookup_mmid, lookup_mmid_name, lookup_mmid_noredir, server_info};
use log::info;
use maud::{html, Markup, PreEscaped};
use pages::{api_info, footer, head};
use rocket::{
    data::{Limits, ToByteUnit}, form::Form, fs::TempFile, get, http::ContentType, post, response::content::{RawCss, RawJavaScript}, routes, serde::{json::Json, Serialize}, tokio, Config, FromForm, State
};
use settings::Settings;
use strings::{parse_time_string, to_pretty_time};
use utils::hash_file;
use uuid::Uuid;

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
        script src="./request.js" { }

        center {
            h1 { "Confetti-Box 🎉" }
            h2 { "Files up to " (settings.max_filesize.bytes()) " in size are allowed!" }
            hr;
            button.main_file_upload #fileButton onclick="document.getElementById('fileInput').click()" {
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
                input #fileDuration type="text" name="duration" minlength="2"
                    maxlength="7" value=(settings.duration.default.num_seconds().to_string() + "s") style="display:none;";
                input #fileInput type="file" name="fileUpload" multiple
                    onchange="formSubmit(this.parentNode)" data-max-filesize=(settings.max_filesize) style="display:none;";
            }
            hr;

            h3 { "Uploaded Files" }
            div #uploadedFilesDisplay {

            }

            hr;
            (footer())
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
    // Ensure the expiry time is valid, if not return an error
    let expire_time = if let Ok(t) = parse_time_string(&file_data.expire_time) {
        if settings.duration.restrict_to_allowed && !settings.duration.allowed.contains(&t) {
            return Ok(Json(ClientResponse::failure("Duration not allowed")));
        }

        if t > settings.duration.maximum {
            return Ok(Json(ClientResponse::failure("Duration larger than max")));
        }

        t
    } else {
        return Ok(Json(ClientResponse::failure("Duration invalid")));
    };

    let raw_name = file_data
        .file
        .raw_name()
        .unwrap()
        .dangerous_unsafe_unsanitized_raw()
        .as_str()
        .to_string();

    // Get temp path for the file
    let temp_filename = settings.temp_dir.join(Uuid::new_v4().to_string());
    file_data.file.persist_to(&temp_filename).await?;

    // Get hash and random identifier
    let file_mmid = Mmid::new();
    let file_hash = hash_file(&temp_filename).await?;

    // Process filetype
    let file_type = file_format::FileFormat::from_file(&temp_filename)?;

    let constructed_file = MochiFile::new_with_expiry(
        file_mmid.clone(),
        raw_name,
        file_type.extension(),
        file_hash,
        expire_time,
    );

    // Move it to the new proper place
    std::fs::rename(temp_filename, settings.file_dir.join(file_hash.to_string()))?;

    db.write().unwrap().insert(&file_mmid, constructed_file.clone());

    Ok(Json(ClientResponse {
        status: true,
        name: constructed_file.name().clone(),
        mmid: Some(file_mmid),
        hash: file_hash.to_string(),
        expires: Some(constructed_file.expiry()),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mmid: Option<Mmid>,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,
}

impl ClientResponse {
    fn failure(response: &'static str) -> Self {
        Self {
            status: false,
            response,
            ..Default::default()
        }
    }
}

#[rocket::main]
async fn main() {
    // Get or create config file
    let config = Settings::open(&"./settings.toml").expect("Could not open settings file");

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
        let file_path = config.file_dir.clone();
        async move { clean_loop(cleaner_db, file_path, rx, TimeDelta::minutes(2)).await }
    });

    let rocket = rocket::build()
        .mount(
            config.server.root_path.clone() + "/",
            routes![
                home,
                api_info,
                handle_upload,
                form_handler_js,
                stylesheet,
                server_info,
                favicon,
                lookup_mmid,
                lookup_mmid_noredir,
                lookup_mmid_name,
            ],
        )
        .manage(database)
        .manage(config)
        .configure(rocket_config)
        .launch()
        .await;

    // Ensure the server gracefully shuts down
    rocket.expect("Server failed to shutdown gracefully");

    info!("Stopping database cleaning thread...");
    shutdown
        .send(())
        .await
        .expect("Failed to stop cleaner thread.");
    info!("Stopping database cleaning thread completed successfully.");

    info!("Saving database on shutdown...");
    local_db.write().unwrap().save();
    info!("Saving database completed successfully.");
}
