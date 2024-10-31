pub mod database;
pub mod endpoints;
pub mod pages;
pub mod resources;
pub mod settings;
pub mod strings;
pub mod utils;

use std::{io::{self, ErrorKind}, sync::{Arc, RwLock}};

use crate::{
    pages::{footer, head},
    settings::Settings,
    strings::to_pretty_time,
};
use chrono::Utc;
use database::{Chunkbase, ChunkedInfo, Mmid, MochiFile, Mochibase};
use maud::{html, Markup, PreEscaped};
use rocket::{
    data::{ByteUnit, ToByteUnit}, get, post, serde::{json::Json, Serialize}, tokio::{fs, io::{AsyncSeekExt, AsyncWriteExt}}, Data, State
};
use utils::hash_file;
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
                maxlength="7" value=(settings.duration.default.num_seconds().to_string()) style="display:none;";
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

/*
#[derive(Debug, FromForm)]
pub struct Upload<'r> {
    #[field(name = "fileUpload")]
    file: TempFile<'r>,
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

/// Handle a file upload and store it
#[post("/upload?<expire_time>", data = "<file_data>")]
pub async fn handle_upload(
    expire_time: String,
    mut file_data: Form<Upload<'_>>,
    db: &State<Arc<RwLock<Mochibase>>>,
    settings: &State<Settings>,
) -> Result<Json<ClientResponse>, std::io::Error> {
    let current = Utc::now();
    // Ensure the expiry time is valid, if not return an error
    let expire_time = if let Ok(t) = parse_time_string(&expire_time) {
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
    let file_mmid = Mmid::new_random();
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
        expiry,
    );

    // If the hash does not exist in the database,
    // move the file to the backend, else, delete it
    if db.read().unwrap().get_hash(&file_hash).is_none() {
        std::fs::rename(temp_filename, settings.file_dir.join(file_hash.to_string()))?;
    } else {
        std::fs::remove_file(temp_filename)?;
    }

    db.write()
        .unwrap()
        .insert(&file_mmid, constructed_file.clone());

    Ok(Json(ClientResponse {
        status: true,
        name: constructed_file.name().clone(),
        mmid: Some(constructed_file.mmid().clone()),
        hash: constructed_file.hash().to_string(),
        expires: Some(constructed_file.expiry()),
        ..Default::default()
    }))
}
*/

#[derive(Serialize, Default)]
pub struct ChunkedResponse {
    status: bool,
    message: String,

    /// UUID used for associating the chunk with the final file
    #[serde(skip_serializing_if = "Option::is_none")]
    uuid: Option<Uuid>,

    /// Valid max chunk size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk_size: Option<ByteUnit>,
}

impl ChunkedResponse {
    fn failure(message: &str) -> Self {
        Self {
            status: false,
            message: message.to_string(),
            ..Default::default()
        }
    }
}

/// Start a chunked upload. Response contains all the info you need to continue
/// uploading chunks.
#[post("/upload/chunked", data = "<file_info>", rank = 2)]
pub async fn chunked_upload_start(
    db: &State<Arc<RwLock<Chunkbase>>>,
    settings: &State<Settings>,
    mut file_info: Json<ChunkedInfo>,
) -> Result<Json<ChunkedResponse>, std::io::Error> {
    let uuid = Uuid::new_v4();
    file_info.path = settings
        .temp_dir
        .join(uuid.to_string());

    // Perform some sanity checks
    if file_info.size > settings.max_filesize {
        return Ok(Json(ChunkedResponse::failure("File too large")));
    }
    if settings.duration.restrict_to_allowed && !settings.duration.allowed.contains(&file_info.expire_duration) {
        return Ok(Json(ChunkedResponse::failure("Duration not allowed")));
    }
    if file_info.expire_duration > settings.duration.maximum {
        return Ok(Json(ChunkedResponse::failure("Duration too large")));
    }

    db.write()
        .unwrap()
        .mut_chunks()
        .insert(uuid, file_info.into_inner());

    Ok(Json(ChunkedResponse {
        status: true,
        message: "".into(),
        uuid: Some(uuid),
        chunk_size: Some(100.megabytes()),
    }))
}

#[post("/upload/chunked?<uuid>&<offset>", data = "<data>", rank = 1)]
pub async fn chunked_upload_continue(
    chunk_db: &State<Arc<RwLock<Chunkbase>>>,
    data: Data<'_>,
    uuid: String,
    offset: u64,
) -> Result<(), io::Error> {
    let uuid = Uuid::parse_str(&uuid).map_err(|e| io::Error::other(e))?;
    let data_stream = data.open(101.megabytes());

    let chunked_info = match chunk_db.read().unwrap().chunks().get(&uuid) {
        Some(s) => s.clone(),
        None => return Err(io::Error::other("Invalid UUID")),
    };

    let mut file = if !chunked_info.path.try_exists().is_ok_and(|e| e) {
        fs::File::create_new(&chunked_info.path).await?
    } else {
        fs::File::options()
            .read(true)
            .write(true)
            .truncate(false)
            .open(&chunked_info.path)
            .await?
    };

    if offset > chunked_info.size {
        return Err(io::Error::new(ErrorKind::InvalidInput, "The seek position is larger than the file size"))
    }

    file.seek(io::SeekFrom::Start(offset)).await?;
    data_stream.stream_to(&mut file).await?.written;
    file.flush().await?;
    let position = file.stream_position().await?;

    if position > chunked_info.size {
        chunk_db.write()
            .unwrap()
            .mut_chunks()
            .remove(&uuid);
        return Err(io::Error::other("File larger than expected"))
    }

    Ok(())
}

/// Finalize a chunked upload
#[get("/upload/chunked?<uuid>&finish", rank = 3)]
pub async fn chunked_upload_finish(
    main_db: &State<Arc<RwLock<Mochibase>>>,
    chunk_db: &State<Arc<RwLock<Chunkbase>>>,
    settings: &State<Settings>,
    uuid: String,
) -> Result<Json<MochiFile>, io::Error> {
    let now = Utc::now();
    let uuid = Uuid::parse_str(&uuid).map_err(|e| io::Error::other(e))?;
    let chunked_info = match chunk_db.read().unwrap().chunks().get(&uuid) {
        Some(s) => s.clone(),
        None => return Err(io::Error::other("Invalid UUID")),
    };

    // Remove the finished chunk from the db
    chunk_db.write()
        .unwrap()
        .mut_chunks()
        .remove(&uuid)
        .unwrap();

    if !chunked_info.path.try_exists().is_ok_and(|e| e) {
        return Err(io::Error::other("File does not exist"))
    }

    let hash = hash_file(&chunked_info.path).await?;
    let mmid = Mmid::new_random();
    let file_type = file_format::FileFormat::from_file(&chunked_info.path)?;

    // If the hash does not exist in the database,
    // move the file to the backend, else, delete it
    if main_db.read().unwrap().get_hash(&hash).is_none() {
        std::fs::rename(chunked_info.path, settings.file_dir.join(hash.to_string()))?;
    } else {
        std::fs::remove_file(chunked_info.path)?;
    }

    let constructed_file = MochiFile::new(
        mmid.clone(),
        chunked_info.name,
        file_type.media_type().to_string(),
        hash,
        now,
        now + chunked_info.expire_duration
    );

    main_db.write().unwrap().insert(&mmid, constructed_file.clone());

    Ok(Json(constructed_file))
}
