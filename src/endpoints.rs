use std::sync::{Arc, RwLock};

use rocket::{
    fs::NamedFile, get, serde::{self, json::Json}, State
};
use serde::Serialize;

use crate::{database::Database, settings::Settings};

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

/// Look up the hash of a file to find it. This only returns the first
/// hit for a hash, so different filenames may not be found.
#[get("/f/<id>")]
pub async fn lookup(
    db: &State<Arc<RwLock<Database>>>,
    settings: &State<Settings>,
    id: &str
) -> Option<NamedFile> {
    dbg!(db.read().unwrap());
    let entry = if let Some(e) = db.read().unwrap().get(&id.into()).cloned() {
        e
    } else {
        return None
    };

    NamedFile::open(settings.file_dir.join(entry.hash().to_string())).await.ok()
}
