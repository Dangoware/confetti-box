use std::{collections::HashMap, fs::{self, File}, path::{Path, PathBuf}, sync::{Arc, RwLock}, time::Duration};

use bincode::{config::Configuration, decode_from_std_read, encode_into_std_write, Decode, Encode};
use chrono::{DateTime, TimeDelta, Utc};
use blake3::Hash;
use log::{info, warn};
use rocket::{serde::{Deserialize, Serialize}, tokio::{select, sync::mpsc::Receiver, time}};

const BINCODE_CFG: Configuration = bincode::config::standard();

#[derive(Debug, Clone)]
#[derive(Decode, Encode)]
pub struct Database {
    path: PathBuf,
    #[bincode(with_serde)]
    pub files: HashMap<MochiKey, MochiFile>
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: &P) -> Self {
        let mut file = File::create_new(path).expect("Could not create database!");

        let output = Self {
            path: path.as_ref().to_path_buf(),
            files: HashMap::new()
        };

        encode_into_std_write(&output, &mut file, BINCODE_CFG).expect("Could not write database!");

        output
    }

    pub fn open<P: AsRef<Path>>(path: &P) -> Self {
        if !path.as_ref().exists() {
            Self::new(path)
        } else {
            let mut file = File::open(path).expect("Could not get database file!");
            decode_from_std_read(&mut file, BINCODE_CFG).expect("Could not decode database")
        }
    }

    pub fn save(&self) {
        let mut out_path = self.path.clone();
        out_path.set_extension(".bkp");
        let mut file = File::create(&out_path).expect("Could not save!");
        encode_into_std_write(self, &mut file, BINCODE_CFG).expect("Could not write out!");

        fs::rename(out_path, &self.path).unwrap();
    }
}

#[derive(Debug, Clone)]
#[derive(Decode, Encode)]
#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct MochiFile {
    /// The original name of the file
    name: String,

    /// Size of the file in bytes
    size: usize,

    /// The location on disk (for deletion and management)
    filename: PathBuf,

    /// The hashed contents of the file as a Blake3 hash
    #[bincode(with_serde)]
    hash: Hash,

    /// The datetime when the file was uploaded
    #[bincode(with_serde)]
    upload_datetime: DateTime<Utc>,

    /// The datetime when the file is set to expire
    #[bincode(with_serde)]
    expiry_datetime: DateTime<Utc>,
}

impl MochiFile {
    /// Create a new file that expires in `expiry`.
    pub fn new_with_expiry(
        name: &str,
        size: usize,
        hash: Hash,
        filename: PathBuf,
        expire_duration: TimeDelta
    ) -> Self {
        let current = Utc::now();
        let expiry = current + expire_duration;

        Self {
            name: name.to_string(),
            size,
            filename,
            hash,
            upload_datetime: current,
            expiry_datetime: expiry,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn path(&self) -> &PathBuf {
        &self.filename
    }

    pub fn get_key(&self) -> MochiKey {
        MochiKey {
            name: self.name.clone(),
            hash: self.hash
        }
    }

    pub fn get_expiry(&self) -> DateTime<Utc> {
        self.expiry_datetime
    }

    pub fn expired(&self) -> bool {
        let datetime = Utc::now();
        datetime > self.expiry_datetime
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(Decode, Encode)]
#[derive(Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct MochiKey {
    name: String,
    #[bincode(with_serde)]
    hash: Hash,
}

/// Clean the database. Removes files which are past their expiry
/// [`chrono::DateTime`]. Also removes files which no longer exist on the disk.
fn clean_database(db: &Arc<RwLock<Database>>) {
    let mut database = db.write().unwrap();
    let files_to_remove: Vec<_> = database.files.iter().filter_map(|e| {
        if e.1.expired() {
            // Check if the entry has expired
            Some((e.0.clone(), e.1.clone()))
        } else if !e.1.path().try_exists().is_ok_and(|r| r) {
            // Check if the entry exists
            Some((e.0.clone(), e.1.clone()))
        } else {
            None
        }
    }).collect();

    let mut expired = 0;
    let mut missing = 0;
    for file in &files_to_remove {
        let path = file.1.path();
        // If the path does not exist, there's no reason to try to remove it.
        if path.try_exists().is_ok_and(|r| r) {
            match fs::remove_file(path) {
                Ok(_) => (),
                Err(e) => warn!("Failed to delete path at {:?}: {e}", path),
            }
            expired += 1;
        } else {
            missing += 1
        }

        database.files.remove(&file.0);
    }

    info!("{} expired and {} missing items cleared from database", expired, missing);
    database.save();
}

/// A loop to clean the database periodically.
pub async fn clean_loop(
    db: Arc<RwLock<Database>>,
    mut shutdown_signal: Receiver<()>,
    interval: Duration,
) {
    let mut interval = time::interval(interval);

    loop {
        select! {
            _ = interval.tick() => clean_database(&db),
            _ = shutdown_signal.recv() => break,
        };
    }
}
