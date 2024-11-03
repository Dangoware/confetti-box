pub mod database;
pub mod endpoints;
pub mod pages;
pub mod resources;
pub mod settings;
pub mod strings;
pub mod utils;

use std::{
    io::{self, ErrorKind},
    sync::{Arc, RwLock},
};

use crate::{
    pages::{footer, head},
    settings::Settings,
    strings::to_pretty_time,
};
use chrono::{TimeDelta, Utc};
use database::{Chunkbase, ChunkedInfo, Mmid, MochiFile, Mochibase};
use maud::{html, Markup, PreEscaped};
use rocket::{
    data::ToByteUnit,
    get, post,
    serde::{json::Json, Serialize},
    tokio::{
        fs,
        io::{AsyncSeekExt, AsyncWriteExt},
    },
    Data, State,
};
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

#[derive(Serialize, Default)]
pub struct ChunkedResponse {
    status: bool,
    message: String,

    /// UUID used for associating the chunk with the final file
    #[serde(skip_serializing_if = "Option::is_none")]
    uuid: Option<Uuid>,

    /// Valid max chunk size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk_size: Option<u64>,
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
#[post("/upload/chunked", data = "<file_info>")]
pub async fn chunked_upload_start(
    db: &State<Arc<RwLock<Chunkbase>>>,
    settings: &State<Settings>,
    mut file_info: Json<ChunkedInfo>,
) -> Result<Json<ChunkedResponse>, std::io::Error> {
    let uuid = Uuid::new_v4();
    file_info.path = settings.temp_dir.join(uuid.to_string());

    // Perform some sanity checks
    if file_info.size > settings.max_filesize {
        return Ok(Json(ChunkedResponse::failure("File too large")));
    }
    if settings.duration.restrict_to_allowed
        && !settings
            .duration
            .allowed
            .contains(&file_info.expire_duration)
    {
        return Ok(Json(ChunkedResponse::failure("Duration not allowed")));
    }
    if file_info.expire_duration > settings.duration.maximum {
        return Ok(Json(ChunkedResponse::failure("Duration too large")));
    }

    fs::File::create_new(&file_info.path).await?;

    db.write().unwrap().mut_chunks().insert(
        uuid,
        (Utc::now() + TimeDelta::seconds(30), file_info.into_inner()),
    );

    Ok(Json(ChunkedResponse {
        status: true,
        message: "".into(),
        uuid: Some(uuid),
        chunk_size: Some(settings.chunk_size),
    }))
}

#[post("/upload/chunked/<uuid>?<offset>", data = "<data>")]
pub async fn chunked_upload_continue(
    chunk_db: &State<Arc<RwLock<Chunkbase>>>,
    settings: &State<Settings>,
    data: Data<'_>,
    uuid: &str,
    offset: u64,
) -> Result<(), io::Error> {
    let uuid = Uuid::parse_str(uuid).map_err(io::Error::other)?;
    let data_stream = data.open((settings.chunk_size + 100).bytes());

    let chunked_info = match chunk_db.read().unwrap().chunks().get(&uuid) {
        Some(s) => s.clone(),
        None => return Err(io::Error::other("Invalid UUID")),
    };

    let mut file = fs::File::options()
        .read(true)
        .write(true)
        .truncate(false)
        .open(&chunked_info.1.path)
        .await?;

    if offset > chunked_info.1.size {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "The seek position is larger than the file size",
        ));
    }

    file.seek(io::SeekFrom::Start(offset)).await?;
    data_stream.stream_to(&mut file).await?;
    file.flush().await?;
    let position = file.stream_position().await?;

    if position > chunked_info.1.size {
        chunk_db.write().unwrap().mut_chunks().remove(&uuid);
        return Err(io::Error::other("File larger than expected"));
    }

    Ok(())
}

/// Finalize a chunked upload
#[get("/upload/chunked/<uuid>?finish")]
pub async fn chunked_upload_finish(
    main_db: &State<Arc<RwLock<Mochibase>>>,
    chunk_db: &State<Arc<RwLock<Chunkbase>>>,
    settings: &State<Settings>,
    uuid: &str,
) -> Result<Json<MochiFile>, io::Error> {
    let now = Utc::now();
    let uuid = Uuid::parse_str(uuid).map_err(io::Error::other)?;
    let chunked_info = match chunk_db.read().unwrap().chunks().get(&uuid) {
        Some(s) => s.clone(),
        None => return Err(io::Error::other("Invalid UUID")),
    };

    // Remove the finished chunk from the db
    chunk_db
        .write()
        .unwrap()
        .mut_chunks()
        .remove(&uuid)
        .unwrap();

    if !chunked_info.1.path.try_exists().is_ok_and(|e| e) {
        return Err(io::Error::other("File does not exist"));
    }

    // Get file hash
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap_rayon(&chunked_info.1.path).unwrap();
    let hash = hasher.finalize();
    let new_filename = settings.file_dir.join(hash.to_string());

    // If the hash does not exist in the database,
    // move the file to the backend, else, delete it
    if main_db.read().unwrap().get_hash(&hash).is_none() {
        std::fs::rename(&chunked_info.1.path, &new_filename).unwrap();
    } else {
        std::fs::remove_file(&chunked_info.1.path).unwrap();
    }

    let mmid = Mmid::new_random();
    let file_type = file_format::FileFormat::from_file(&new_filename).unwrap();

    let constructed_file = MochiFile::new(
        mmid.clone(),
        chunked_info.1.name,
        file_type.media_type().to_string(),
        hash,
        now,
        now + chunked_info.1.expire_duration,
    );

    main_db
        .write()
        .unwrap()
        .insert(&mmid, constructed_file.clone());

    Ok(Json(constructed_file))
}
