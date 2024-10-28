use std::sync::{Arc, RwLock};

use rocket::{
    get,
    http::ContentType,
    response::Redirect,
    serde::{self, json::Json},
    tokio::fs::File,
    uri, State,
};
use serde::Serialize;

use crate::{
    database::{Database, Mmid},
    settings::Settings,
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
pub async fn lookup_mmid(db: &State<Arc<RwLock<Database>>>, mmid: &str) -> Option<Redirect> {
    let mmid: Mmid = mmid.try_into().ok()?;
    let entry = db.read().unwrap().get(&mmid).cloned()?;

    Some(Redirect::to(uri!(lookup_mmid_name(
        mmid.to_string(),
        entry.name()
    ))))
}

/// Look up the [`Mmid`] of a file to find it, along with the name of the file
#[get("/f/<mmid>/<name>")]
pub async fn lookup_mmid_name(db: &State<Arc<RwLock<Database>>>,
    settings: &State<Settings>,
    mmid: &str,
    name: &str,
) -> Option<(ContentType, File)> {
    let mmid: Mmid = mmid.try_into().ok()?;

    let entry = db.read().unwrap().get(&mmid).cloned()?;

    // If the name does not match, then this is invalid
    if name != entry.name() {
        return None;
    }

    let file = File::open(settings.file_dir.join(entry.hash().to_string()))
        .await
        .ok()?;

    Some((
        ContentType::from_extension(entry.extension()).unwrap_or(ContentType::Binary),
        file,
    ))
}
