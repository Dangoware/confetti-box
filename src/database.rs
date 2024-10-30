use std::{
    collections::{hash_map::Values, HashMap, HashSet}, ffi::OsStr, fs::{self, File}, io, path::{Path, PathBuf}, sync::{Arc, RwLock}
};

use bincode::{config::Configuration, decode_from_std_read, encode_into_std_write, Decode, Encode};
use blake3::Hash;
use chrono::{DateTime, TimeDelta, Utc};
use log::{error, info, warn};
use rand::distributions::{Alphanumeric, DistString};
use rocket::{
    serde::{Deserialize, Serialize},
    tokio::{select, sync::mpsc::Receiver, time},
};
use serde_with::{serde_as, DisplayFromStr};

const BINCODE_CFG: Configuration = bincode::config::standard();

#[derive(Debug, Clone, Decode, Encode)]
pub struct Mochibase {
    path: PathBuf,

    /// Every hash in the database along with the [`Mmid`]s associated with them
    #[bincode(with_serde)]
    hashes: HashMap<Hash, HashSet<Mmid>>,

    /// All entries in the database
    #[bincode(with_serde)]
    entries: HashMap<Mmid, MochiFile>,
}

impl Mochibase {
    pub fn new<P: AsRef<Path>>(path: &P) -> Result<Self, io::Error> {
        let output = Self {
            path: path.as_ref().to_path_buf(),
            entries: HashMap::new(),
            hashes: HashMap::new(),
        };

        // Save the database initially after creating it
        output.save()?;

        Ok(output)
    }

    /// Open the database from a path
    pub fn open<P: AsRef<Path>>(path: &P) -> Result<Self, io::Error> {
        let file = File::open(path)?;
        let mut lz4_file = lz4_flex::frame::FrameDecoder::new(file);

        decode_from_std_read(&mut lz4_file, BINCODE_CFG)
            .map_err(|e| io::Error::other(format!("failed to open database: {e}")))
    }

    /// Open the database from a path, **or create it if it does not exist**
    pub fn open_or_new<P: AsRef<Path>>(path: &P) -> Result<Self, io::Error> {
        if !path.as_ref().exists() {
            Self::new(path)
        } else {
            Self::open(path)
        }
    }

    /// Save the database to its file
    pub fn save(&self) -> Result<(), io::Error> {
        // Create a file and write the LZ4 compressed stream into it
        let file = File::create(&self.path.with_extension("bkp"))?;
        let mut lz4_file = lz4_flex::frame::FrameEncoder::new(file);
        encode_into_std_write(self, &mut lz4_file, BINCODE_CFG)
            .map_err(|e| io::Error::other(format!("failed to save database: {e}")))?;
        lz4_file.try_finish()?;

        fs::rename(
            self.path.with_extension("bkp"),
            &self.path
        ).unwrap();

        Ok(())
    }

    /// Insert a [`MochiFile`] into the database.
    ///
    /// If the database already contained this value, then `false` is returned.
    pub fn insert(&mut self, mmid: &Mmid, entry: MochiFile) -> bool {
        if let Some(s) = self.hashes.get_mut(&entry.hash) {
            // If the database already contains the hash, make sure the file is unique
            if !s.insert(mmid.clone()) {
                return false;
            }
        } else {
            // If the database does not contain the hash, create a new set for it
            self.hashes
                .insert(entry.hash, HashSet::from([mmid.clone()]));
        }

        self.entries.insert(mmid.clone(), entry.clone());

        true
    }

    /// Remove an [`Mmid`] from the database entirely.
    ///
    /// If the database did not contain this value, then `false` is returned.
    pub fn remove_mmid(&mut self, mmid: &Mmid) -> bool {
        let hash = if let Some(h) = self.entries.get(mmid).map(|e| e.hash) {
            self.entries.remove(mmid);
            h
        } else {
            return false;
        };

        if let Some(s) = self.hashes.get_mut(&hash) {
            s.remove(mmid);
        }

        true
    }

    /// Remove a hash from the database entirely.
    ///
    /// Will not remove (returns [`Some(false)`] if hash contains references.
    pub fn remove_hash(&mut self, hash: &Hash) -> Option<bool> {
        if let Some(s) = self.hashes.get(hash) {
            if s.is_empty() {
                self.hashes.remove(hash);
                Some(true)
            } else {
                Some(false)
            }
        } else {
            None
        }
    }

    /// Checks if a hash contained in the database contains no more [`Mmid`]s.
    pub fn is_hash_empty(&self, hash: &Hash) -> Option<bool> {
        self.hashes.get(hash).map(|s| s.is_empty())
    }

    /// Get an entry by its [`Mmid`]. Returns [`None`] if the value does not exist.
    pub fn get(&self, mmid: &Mmid) -> Option<&MochiFile> {
        self.entries.get(mmid)
    }

    pub fn get_hash(&self, hash: &Hash) -> Option<&HashSet<Mmid>> {
        self.hashes.get(hash)
    }

    pub fn entries(&self) -> Values<'_, Mmid, MochiFile> {
        self.entries.values()
    }
}

/// An entry in the database storing metadata about a file
#[serde_as]
#[derive(Debug, Clone, Decode, Encode, Deserialize, Serialize)]
pub struct MochiFile {
    /// A unique identifier describing this file
    mmid: Mmid,

    /// The original name of the file
    name: String,

    /// The MIME type of the file
    mime_type: String,

    /// The Blake3 hash of the file
    #[bincode(with_serde)]
    #[serde_as(as = "DisplayFromStr")]
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
    pub fn new(
        mmid: Mmid,
        name: String,
        mime_type: String,
        hash: Hash,
        upload: DateTime<Utc>,
        expiry: DateTime<Utc>,
    ) -> Self {
        Self {
            mmid,
            name,
            mime_type,
            hash,
            upload_datetime: upload,
            expiry_datetime: expiry,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn expiry(&self) -> DateTime<Utc> {
        self.expiry_datetime
    }

    pub fn is_expired(&self) -> bool {
        let datetime = Utc::now();
        datetime > self.expiry_datetime
    }

    pub fn hash(&self) -> &Hash {
        &self.hash
    }

    pub fn mmid(&self) -> &Mmid {
        &self.mmid
    }

    pub fn mime_type(&self) -> &String {
        &self.mime_type
    }
}

/// Clean the database. Removes files which are past their expiry
/// [`chrono::DateTime`]. Also removes files which no longer exist on the disk.
fn clean_database(db: &Arc<RwLock<Mochibase>>, file_path: &Path) {
    let mut database = db.write().unwrap();

    // Add expired entries to the removal list
    let files_to_remove: Vec<_> = database
        .entries()
        .filter_map(|e| {
            if e.is_expired() {
                Some((e.mmid().clone(), *e.hash()))
            } else {
                None
            }
        })
        .collect();

    let mut removed_files = 0;
    let mut removed_entries = 0;
    for e in &files_to_remove {
        if database.remove_mmid(&e.0) {
            removed_entries += 1;
        }
        if database.is_hash_empty(&e.1).is_some_and(|b| b) {
            database.remove_hash(&e.1);
            if let Err(e) = fs::remove_file(file_path.join(e.1.to_string())) {
                warn!("Failed to remove expired hash: {}", e);
            } else {
                removed_files += 1;
            }
        }
    }

    info!("Cleaned database.\n\t| Removed {removed_entries} expired entries.\n\t| Removed {removed_files} no longer referenced files.");

    if let Err(e) = database.save() {
        error!("Failed to save database: {e}")
    }
    drop(database); // Just to be sure
}

/// A loop to clean the database periodically.
pub async fn clean_loop(
    db: Arc<RwLock<Mochibase>>,
    file_path: PathBuf,
    mut shutdown_signal: Receiver<()>,
    interval: TimeDelta,
) {
    let mut interval = time::interval(interval.to_std().unwrap());

    loop {
        select! {
            _ = interval.tick() => clean_database(&db, &file_path),
            _ = shutdown_signal.recv() => break,
        };
    }
}

/// A unique identifier for an entry in the database, 8 characters long,
/// consists of ASCII alphanumeric characters (`a-z`, `A-Z`, and `0-9`).
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
#[derive(Decode, Encode)]
#[derive(Deserialize, Serialize)]
pub struct Mmid(String);

impl Mmid {
    /// Create a new random MMID
    pub fn new() -> Self {
        let string = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);

        Self(string)
    }
}

impl TryFrom<&str> for Mmid {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(());
        }

        if value.chars().any(|c| !c.is_ascii_alphanumeric()) {
            return Err(());
        }

        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<&Path> for Mmid {
    type Error = ();

    fn try_from(value: &Path) -> Result<Self, Self::Error> {
        value.as_os_str().try_into()
    }
}

impl TryFrom<&OsStr> for Mmid {
    type Error = ();

    fn try_from(value: &OsStr) -> Result<Self, Self::Error> {
        let string = match value.to_str() {
            Some(p) => p,
            None => return Err(()),
        };

        if string.len() != 8 {
            return Err(());
        }

        if string.chars().any(|c| !c.is_ascii_alphanumeric()) {
            return Err(());
        }

        Ok(Self(string.to_owned()))
    }
}

impl std::fmt::Display for Mmid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
