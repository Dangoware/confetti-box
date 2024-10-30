pub mod database;
pub mod endpoints;
pub mod settings;
pub mod strings;
pub mod utils;
pub mod pages;
pub mod resources;

use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use crate::database::{Mmid, MochiFile, Mochibase};
use maud::{html, Markup, PreEscaped};
use crate::pages::{footer, head};
use rocket::{
    data::ToByteUnit, form::Form, fs::TempFile, get, post, serde::{json::Json, Serialize}, FromForm, State
};
use crate::settings::Settings;
use crate::strings::{parse_time_string, to_pretty_time};
use crate::utils::hash_file;
use uuid::Uuid;

#[get("/")]
pub fn home(settings: &State<Settings>) -> Markup {
    html! {
        (head("Confetti-Box"))
        script src="/resources/request.js" { }

        center {
            h1 { "Confetti-Box 🎉" }
            h2 { "Files up to " (settings.max_filesize.bytes()) " in size are allowed!" }
            hr;
            button.main_file_upload #fileButton onclick="document.getElementById('fileInput').click()" {
                h4 { "Upload File(s)" }
                p { "Click, Paste, or Drag and Drop" }
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
pub struct Upload<'r> {
    #[field(name = "duration")]
    expire_time: String,

    #[field(name = "fileUpload")]
    file: TempFile<'r>,
}

/// Handle a file upload and store it
#[post("/upload", data = "<file_data>")]
pub async fn handle_upload(
    mut file_data: Form<Upload<'_>>,
    db: &State<Arc<RwLock<Mochibase>>>,
    settings: &State<Settings>,
) -> Result<Json<ClientResponse>, std::io::Error> {
    let current = Utc::now();
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

    // Get hash and random identifier and expiry
    let file_mmid = Mmid::new();
    let file_hash = hash_file(&temp_filename).await?;
    let expiry = current + expire_time;

    // Process filetype
    let file_type = file_format::FileFormat::from_file(&temp_filename)?;

    let constructed_file = MochiFile::new(
        file_mmid.clone(),
                                          raw_name,
                                          file_type.media_type().to_string(),
                                          file_hash,
                                          current,
                                          expiry
    );

    // If the hash does not exist in the database,
    // move the file to the backend, else, delete it
    if db.read().unwrap().get_hash(&file_hash).is_none() {
        std::fs::rename(temp_filename, settings.file_dir.join(file_hash.to_string()))?;
    } else {
        std::fs::remove_file(temp_filename)?;
    }

    db.write().unwrap().insert(&file_mmid, constructed_file.clone());

    Ok(Json(ClientResponse {
        status: true,
        name: constructed_file.name().clone(),
            mmid: Some(constructed_file.mmid().clone()),
            hash: constructed_file.hash().to_string(),
            expires: Some(constructed_file.expiry()),
            ..Default::default()
    }))
}

/// A response to the client from the server
#[derive(Serialize, Default, Debug)]
pub struct ClientResponse {
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
