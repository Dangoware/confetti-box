use std::sync::{Arc, RwLock};

use rocket::{
    fs::NamedFile, get, http::ContentType, serde::{self, json::Json}, tokio::fs::File, State
};
use serde::Serialize;

use crate::{database::{Database, Mmid}, settings::Settings};

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

#[derive(Serialize, Debug)]
#[serde(crate = "rocket::serde")]
pub struct ServerInfo {
    max_filesize: u64,
    max_duration: u32,
    default_duration: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    allowed_durations: Vec<u32>,
}

/// Look up the [`Mmid`] of a file to find it.
#[get("/f/<mmid>")]
pub async fn lookup(
    db: &State<Arc<RwLock<Database>>>,
    settings: &State<Settings>,
    mmid: &str
) -> Option<(ContentType, NamedFile)> {
    let mmid: Mmid = match mmid.try_into() {
        Ok(v) => v,
        Err(_) => return None,
    };

    let entry = if let Some(e) = db.read().unwrap().get(&mmid).cloned() {
        e
    } else {
        return None
    };

    let file = NamedFile::open(settings.file_dir.join(entry.hash().to_string())).await.ok()?;

    Some((
        ContentType::from_extension(entry.extension()).unwrap_or(ContentType::Binary),
        file
    ))
}
