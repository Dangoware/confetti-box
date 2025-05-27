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
    strings::pretty_time,
};
use chrono::{TimeDelta, Utc};
use database::{Chunkbase, ChunkedInfo, Mmid, MochiFile, Mochibase};
use maud::{html, Markup, PreEscaped};
use rocket::{
    data::ToByteUnit, futures::{SinkExt as _, StreamExt as _}, get, post, serde::{json::{self, Json}, Serialize}, tokio::{
        fs, io::{AsyncSeekExt, AsyncWriteExt}
    }, Data, State
};
use strings::{BreakStyle, TimeGranularity};
use uuid::Uuid;

#[get("/")]
pub fn home(settings: &State<Settings>) -> Markup {
    html! {
        (head("Confetti-Box"))
        script src="/resources/request.js" { }

        center {
            h1 { "Confetti-Box 🎉" }
            h2 { "Files up to " (settings.max_filesize.bytes()) " in size are allowed!" }
            noscript { "Javascript must be enabled for this site to function!" }
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
                        (PreEscaped(pretty_time(d.num_seconds(), BreakStyle::Break, TimeGranularity::Seconds)))
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
    file_info: Json<ChunkedInfo>,
) -> Result<Json<ChunkedResponse>, std::io::Error> {
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

    let uuid = db.write().unwrap().new_file(
        file_info.0,
        &settings.temp_dir,
        TimeDelta::seconds(30)
    )?;

    Ok(Json(ChunkedResponse {
        status: true,
        message: "".into(),
        uuid: Some(uuid),
        chunk_size: Some(settings.chunk_size),
    }))
}

#[post("/upload/chunked/<uuid>?<chunk>", data = "<data>")]
pub async fn chunked_upload_continue(
    chunk_db: &State<Arc<RwLock<Chunkbase>>>,
    settings: &State<Settings>,
    data: Data<'_>,
    uuid: &str,
    chunk: u64,
) -> Result<(), io::Error> {
    let uuid = Uuid::parse_str(uuid).map_err(io::Error::other)?;
    let data_stream = data.open((settings.chunk_size + 100).bytes());

    let chunked_info = match chunk_db.read().unwrap().get_file(&uuid) {
        Some(s) => s.clone(),
        None => return Err(io::Error::other("Invalid UUID")),
    };

    if chunked_info.1.recieved_chunks.contains(&chunk) {
        return Err(io::Error::new(ErrorKind::Other, "Chunk already uploaded"));
    }

    let mut file = fs::File::options()
        .read(true)
        .write(true)
        .truncate(false)
        .open(&chunked_info.1.path)
        .await?;

    let offset = chunk * settings.chunk_size;
    if (offset > chunked_info.1.size) | (offset > settings.max_filesize) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Invalid chunk number for file",
        ));
    }

    file.seek(io::SeekFrom::Start(offset)).await?;
    let written = data_stream.stream_to(&mut file).await?.written;
    file.flush().await?;
    let position = file.stream_position().await?;

    if written > settings.chunk_size {
        chunk_db.write().unwrap().remove_file(&uuid)?;
        return Err(io::Error::other("Wrote more than one chunk"));
    }
    if position > chunked_info.1.size {
        chunk_db.write().unwrap().remove_file(&uuid)?;
        return Err(io::Error::other("File larger than expected"));
    }

    chunk_db.write().unwrap().add_recieved_chunk(&uuid, chunk);
    chunk_db.write().unwrap().extend_timeout(&uuid, TimeDelta::seconds(30));

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
    let chunked_info = match chunk_db.read().unwrap().get_file(&uuid) {
        Some(s) => s.clone(),
        None => return Err(io::Error::other("Invalid UUID")),
    };

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
    // This also removes it from the chunk database
    if main_db.read().unwrap().get_hash(&hash).is_none() {
        chunk_db.write().unwrap().move_and_remove_file(&uuid, &new_filename)?;
    } else {
        chunk_db.write().unwrap().remove_file(&uuid)?;
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

#[get("/upload/websocket?<name>&<size>&<duration>")]
pub async fn websocket_upload(
    ws: rocket_ws::WebSocket,
    main_db: &State<Arc<RwLock<Mochibase>>>,
    chunk_db: &State<Arc<RwLock<Chunkbase>>>,
    settings: &State<Settings>,
    name: String,
    size: u64,
    duration: i64, // Duration in seconds
) -> Result<rocket_ws::Channel<'static>, Json<ChunkedResponse>> {
    let max_filesize = settings.max_filesize;
    let expire_duration = TimeDelta::seconds(duration);
    if size > max_filesize {
        return Err(Json(ChunkedResponse::failure("File too large")));
    }
    if settings.duration.restrict_to_allowed
        && !settings
            .duration
            .allowed
            .contains(&expire_duration)
    {
        return Err(Json(ChunkedResponse::failure("Duration not allowed")));
    }
    if expire_duration > settings.duration.maximum {
        return Err(Json(ChunkedResponse::failure("Duration too large")));
    }

    let file_info = ChunkedInfo {
        name,
        size,
        expire_duration,
        ..Default::default()
    };

    let uuid = chunk_db.write().unwrap().new_file(
        file_info,
        &settings.temp_dir,
        TimeDelta::seconds(30)
    ).map_err(|e| Json(ChunkedResponse::failure(e.to_string().as_str())))?;
    let info = chunk_db.read().unwrap().get_file(&uuid).unwrap().clone();

    let chunk_db = Arc::clone(chunk_db);
    let main_db = Arc::clone(main_db);
    let file_dir = settings.file_dir.clone();
    let mut file = fs::File::create(&info.1.path).await.unwrap();

    Ok(ws.channel(move |mut stream| Box::pin(async move {
        let mut offset = 0;
        let mut hasher = blake3::Hasher::new();
        while let Some(message) = stream.next().await {
            if let Ok(m) = message.as_ref() {
                if m.is_empty() {
                    // We're finished here
                    break;
                }
            }

            let message = message.unwrap().into_data();
            offset += message.len() as u64;
            if (offset > info.1.size) | (offset > max_filesize) {
                break
            }

            hasher.update(&message);

            stream.send(rocket_ws::Message::binary(offset.to_le_bytes().as_slice())).await.unwrap();

            file.write_all(&message).await.unwrap();

            chunk_db.write().unwrap().extend_timeout(&uuid, TimeDelta::seconds(30));
        }

        let now = Utc::now();
        let hash = hasher.finalize();
        let new_filename = file_dir.join(hash.to_string());

        // If the hash does not exist in the database,
        // move the file to the backend, else, delete it
        // This also removes it from the chunk database
        if main_db.read().unwrap().get_hash(&hash).is_none() {
            chunk_db.write().unwrap().move_and_remove_file(&uuid, &new_filename)?;
        } else {
            chunk_db.write().unwrap().remove_file(&uuid)?;
        }

        let mmid = Mmid::new_random();
        let file_type = file_format::FileFormat::from_file(&new_filename).unwrap();

        let constructed_file = MochiFile::new(
            mmid.clone(),
            info.1.name,
            file_type.media_type().to_string(),
            hash,
            now,
            now + info.1.expire_duration,
        );

        main_db
            .write()
            .unwrap()
            .insert(&mmid, constructed_file.clone());

        file.flush().await.unwrap();

        stream.send(rocket_ws::Message::Text(json::serde_json::ser::to_string(&constructed_file).unwrap())).await?;
        stream.close(None).await?;

        Ok(())
    })))
}
