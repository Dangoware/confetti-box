use std::{
    str::FromStr,
    sync::{Arc, RwLock},
};

use chrono::Utc;
use maud::{html, Markup, DOCTYPE};
use rocket::{
    get, http::ContentType, response::{self, Redirect, Responder, Response}, serde::{self, json::Json}, tokio::{self, fs::File}, uri, Request, State
};
use serde::Serialize;

use crate::{
    database::{Mmid, MochiFile, Mochibase}, settings::Settings, strings::{to_pretty_size, to_pretty_time, BreakStyle, TimeGranularity}
};

/// An endpoint to obtain information about the server's capabilities
#[get("/info")]
pub fn server_info(settings: &State<Settings>) -> Json<ServerInfo> {
    Json(ServerInfo {
        max_filesize: settings.max_filesize,
        max_duration: settings.duration.maximum.num_seconds() as u32,
        default_duration: settings.duration.default.num_seconds() as u32,
        allowed_durations: settings
            .duration
            .allowed
            .clone()
            .into_iter()
            .map(|t| t.num_seconds() as u32)
            .collect(),
    })
}

/// Get information about a file
#[get("/info/<mmid>")]
pub async fn file_info(db: &State<Arc<RwLock<Mochibase>>>, mmid: &str) -> Option<Json<MochiFile>> {
    let mmid: Mmid = mmid.try_into().ok()?;
    let entry = db.read().unwrap().get(&mmid)?;

    Some(Json(entry))
}

#[get("/info/<mmid>?opengraph")]
pub async fn file_info_opengraph(
    db: &State<Arc<RwLock<Mochibase>>>,
    settings: &State<Settings>,
    mmid: &str,
) -> Option<Markup> {
    let mmid: Mmid = mmid.try_into().ok()?;
    let entry = db.read().unwrap().get(&mmid)?;

    let file = File::open(settings.file_dir.join(entry.hash().to_string()))
        .await
        .ok()?;

    let size = to_pretty_size(file.metadata().await.ok()?.len());

    let seconds_till_expiry = entry.expiry().and_utc().signed_duration_since(Utc::now()).num_seconds();
    let expiry = to_pretty_time(seconds_till_expiry as u32, BreakStyle::Space, TimeGranularity::Minutes);

    let title = entry.name().clone() + " - " + &size + " - " + &expiry;

    let url = uri!(lookup_mmid_name(
        mmid.to_string(),
        entry.name()
    )).to_string();

    Some(html! {
        (DOCTYPE)
        meta charset="UTF-8";
        title { (title) }
        link rel="icon" type="image/svg+xml" href="/favicon.svg";
        meta property="og:title" content=(title);
        meta property="twitter:title" content=(title);
        meta property="og:description" content={"Size: " (size) ", expires in " (expiry)};

        body {
            script {
                "window.location.href = '" (url) "';"
            }
        }
    })
}

#[derive(Serialize, Debug)]
#[serde(crate = "rocket::serde")]
pub struct ServerInfo {
    max_filesize: u64,
    max_duration: u32,
    default_duration: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    allowed_durations: Vec<u32>,
}

#[get("/f/<mmid>")]
pub async fn lookup_mmid(db: &State<Arc<RwLock<Mochibase>>>, mmid: &str) -> Option<Redirect> {
    let mmid: Mmid = mmid.try_into().ok()?;
    let entry = db.read().unwrap().get(&mmid)?;

    Some(Redirect::to(uri!(lookup_mmid_name(
        mmid.to_string(),
        entry.name()
    ))))
}

#[get("/f/<mmid>?noredir&<download>")]
pub async fn lookup_mmid_noredir(
    db: &State<Arc<RwLock<Mochibase>>>,
    settings: &State<Settings>,
    mmid: &str,
    download: bool,
) -> Option<FileDownloader> {
    let mmid: Mmid = mmid.try_into().ok()?;
    let entry = db.read().unwrap().get(&mmid)?;

    let file = File::open(settings.file_dir.join(entry.hash().to_string()))
        .await
        .ok()?;

    Some(FileDownloader {
        inner: file,
        filename: entry.name().clone(),
        content_type: ContentType::from_str(entry.mime_type()).unwrap_or(ContentType::Binary),
        disposition: download
    })
}

pub struct FileDownloader {
    inner: tokio::fs::File,
    filename: String,
    content_type: ContentType,
    disposition: bool,
}

impl<'r> Responder<'r, 'r> for FileDownloader {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'r> {
        let mut resp = Response::build();
        resp.streamed_body(self.inner)
            .header(self.content_type);

        if self.disposition {
            resp.raw_header(
                "Content-Disposition",
                format!(
                    "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                    unidecode::unidecode(&self.filename),
                    urlencoding::encode(&self.filename)
                )
            );
        }

        resp.ok()
    }
}


#[get("/f/<mmid>/<name>")]
pub async fn lookup_mmid_name(
    db: &State<Arc<RwLock<Mochibase>>>,
    settings: &State<Settings>,
    mmid: &str,
    name: &str,
) -> Option<(ContentType, File)> {
    let mmid: Mmid = mmid.try_into().ok()?;
    let entry = db.read().unwrap().get(&mmid)?;

    // If the name does not match, then this is invalid
    if name != entry.name() {
        return None;
    }

    let file = File::open(settings.file_dir.join(entry.hash().to_string()))
        .await
        .ok()?;

    Some((
        ContentType::from_str(entry.mime_type()).unwrap_or(ContentType::Binary),
        file,
    ))
}
