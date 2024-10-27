use std::sync::{Arc, RwLock};

use rocket::{
    get,
    http::RawStr,
    response::{status::NotFound, Redirect},
    serde::{self, json::Json},
    State,
};
use serde::Serialize;

use crate::{database::Database, get_id, settings::Settings};

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
pub fn lookup(db: &State<Arc<RwLock<Database>>>, id: &str) -> Result<Redirect, NotFound<()>> {
    for file in db.read().unwrap().files.values() {
        if file.hash().to_hex()[0..10].to_string() == id {
            let filename = get_id(file.name(), *file.hash());
            let filename = RawStr::new(&filename).percent_encode().to_string();
            return Ok(Redirect::to(format!("/files/{}", filename)));
        }
    }

    Err(NotFound(()))
}
