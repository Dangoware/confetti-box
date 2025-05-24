pub mod schema;

use std::{
    collections::{HashMap, HashSet}, ffi::OsStr, fs::{self}, io::{self}, path::{Path, PathBuf}, str::FromStr, sync::{Arc, Mutex, RwLock}
};

use blake3::Hash;
use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use dotenvy::dotenv;
use log::{error, info, warn};
use rand::distributions::{Alphanumeric, DistString};
use rocket::{
    form::{self, FromFormField, ValueField},
    serde::{Deserialize, Serialize},
};
use serde_with::serde_as;
use uuid::Uuid;

use diesel::prelude::*;

pub struct Mochibase {
    path: PathBuf,
    /// connection to the db
    db: Arc<Mutex<SqliteConnection>>,
}

impl Mochibase {
    /// Open the database from a path, **or create it if it does not exist**
    pub fn open_or_new<P: AsRef<str>>(path: &P) -> Result<Self, io::Error> {
        dotenv().ok();
        let connection = SqliteConnection::establish(path.as_ref())
            .unwrap_or_else(|e| panic!("Failed to connect, error: {}", e));
        Ok(
            Self {
                path: PathBuf::from_str(path.as_ref()).unwrap(),
                db: Arc::new(Mutex::new(connection))
            }
        )
    }

    /// Insert a [`MochiFile`] into the database.
    ///
    /// If the database already contained this value, then `false` is returned.
    pub fn insert(&mut self, mmid_: &Mmid, entry: MochiFile) -> bool {
        use schema::mochifiles::dsl::*;

        let hash_matched_mmids: Vec<Mmid> = mochifiles
            .filter(hash.eq(entry.hash()))
            .select(mmid)
            .load(&mut *self.db.lock().unwrap())
            .expect("Error getting mmids");

        // If the database already contains the hash, make sure the file is unique
        if hash_matched_mmids.contains(mmid_) {
                return false;
        }
        entry.insert_into(mochifiles).on_conflict_do_nothing();

        true
    }

    /// Remove an [`Mmid`] from the database entirely.
    ///
    /// If the database did not contain this value, then `false` is returned.
    pub fn remove_mmid(&mut self, mmid_: &Mmid) -> bool {
        use schema::mochifiles::dsl::*;

        if diesel::delete(mochifiles.filter(mmid.eq(mmid_))).execute(&mut *self.db.lock().unwrap()).expect("Error deleting posts") > 0 {
            true
        } else {
            false
        }
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
    pub fn get(&self, mmid_: &Mmid) -> Option<MochiFile> {
        use schema::mochifiles::dsl::*;
        mochifiles.filter(mmid.eq(mmid_)).select(MochiFile::as_select()).load(&mut *self.db.lock().unwrap()).unwrap().get(0).map(|f| f.clone())
    }

    pub fn get_hash(&self, hash_: &String) -> Option<Vec<MochiFile>> {
        use schema::mochifiles::dsl::*;
        let files = mochifiles.filter(hash.eq(hash_)).select(MochiFile::as_select()).load(&mut *self.db.lock().unwrap()).expect("failed to load mochifiles by hash");
        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    }

    pub fn entries(&self) -> Vec<MochiFile> {
        use schema::mochifiles::dsl::*;
        mochifiles.select(MochiFile::as_select()).load(&mut *self.db.lock().unwrap()).expect("failed to load all mochifiles")
    }
}

/// An entry in the database storing metadata about a file
#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::database::schema::mochifiles)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MochiFile {
    /// A unique identifier describing this file
    mmid: Mmid,

    /// The original name of the file
    name: String,

    /// The MIME type of the file
    mime_type: String,

    /// The Blake3 hash of the file
    hash: String,

    /// The datetime when the file was uploaded
    upload_datetime: chrono::NaiveDateTime,

    /// The datetime when the file is set to expire
    expiry_datetime: chrono::NaiveDateTime,
}


impl MochiFile {
    /// Create a new file that expires in `expiry`.
    pub fn new(
        mmid: Mmid,
        name: String,
        mime_type: String,
        hash: String,
        upload: NaiveDateTime,
        expiry: NaiveDateTime,
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

    pub fn expiry(&self) -> NaiveDateTime {
        self.expiry_datetime
    }

    pub fn is_expired(&self) -> bool {
        let datetime = Utc::now();
        datetime > self.expiry_datetime.and_utc()
    }

    pub fn hash(&self) -> &String {
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
pub fn clean_database(db: &Arc<RwLock<Mochibase>>, file_path: &Path) {
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

/// A unique identifier for an entry in the database, 8 characters long,
/// consists of ASCII alphanumeric characters (`a-z`, `A-Z`, and `0-9`).
#[derive(diesel_derive_newtype::DieselNewType)]
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct Mmid(String);

impl Mmid {
    /// Create a new random MMID
    pub fn new_random() -> Self {
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

#[rocket::async_trait]
impl<'r> FromFormField<'r> for Mmid {
    fn from_value(field: ValueField<'r>) -> form::Result<'r, Self> {
        Ok(Self::try_from(field.value).map_err(|_| form::Error::validation("Invalid MMID"))?)
    }
}

/// An in-memory database for partially uploaded chunks of files
#[derive(Default, Debug)]
pub struct Chunkbase {
    chunks: HashMap<Uuid, (DateTime<Utc>, ChunkedInfo)>,
}

impl Chunkbase {
    /// Delete all temporary chunk files
    pub fn delete_all(&mut self) -> Result<(), io::Error> {
        for (_timeout, chunk) in self.chunks.values() {
            fs::remove_file(&chunk.path)?;
        }

        self.chunks.clear();

        Ok(())
    }

    pub fn delete_timed_out(&mut self) -> Result<(), io::Error> {
        let now = Utc::now();
        self.chunks.retain(|_u, (t, c)| {
            if *t <= now {
                let _ = fs::remove_file(&c.path);
                false
            } else {
                true
            }
        });

        Ok(())
    }

    pub fn new_file<P: AsRef<Path>>(&mut self, mut info: ChunkedInfo, temp_dir: &P, timeout: TimeDelta) -> Result<Uuid, io::Error> {
        let uuid = Uuid::new_v4();
        let expire = Utc::now() + timeout;
        info.path = temp_dir.as_ref().join(uuid.to_string());

        self.chunks.insert(uuid, (expire, info.clone()));

        fs::File::create_new(&info.path)?;

        Ok(uuid)
    }

    pub fn get_file(&self, uuid: &Uuid) -> Option<&(DateTime<Utc>, ChunkedInfo)> {
        self.chunks.get(uuid)
    }

    pub fn remove_file(&mut self, uuid: &Uuid) -> Result<bool, io::Error> {
        let item = match self.chunks.remove(uuid) {
            Some(i) => i,
            None => return Ok(false),
        };

        fs::remove_file(item.1.path)?;

        Ok(true)
    }

    pub fn move_and_remove_file<P: AsRef<Path>>(&mut self, uuid: &Uuid, new_location: &P) -> Result<bool, io::Error> {
        let item = match self.chunks.remove(uuid) {
            Some(i) => i,
            None => return Ok(false),
        };

        fs::rename(item.1.path, new_location)?;

        Ok(true)
    }

    pub fn extend_timeout(&mut self, uuid: &Uuid, timeout: TimeDelta) -> bool {
        let item = match self.chunks.get_mut(uuid) {
            Some(i) => i,
            None => return false,
        };

        item.0 = Utc::now() + timeout;

        true
    }

    pub fn add_recieved_chunk(&mut self, uuid: &Uuid, chunk: u64) -> bool {
        let item = match self.chunks.get_mut(uuid) {
            Some(i) => i,
            None => return false,
        };

        item.1.recieved_chunks.insert(chunk)
    }
}

/// Information about how to manage partially uploaded chunks of files
#[serde_as]
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
pub struct ChunkedInfo {
    pub name: String,
    pub size: u64,
    #[serde_as(as = "serde_with::DurationSeconds<i64>")]
    pub expire_duration: TimeDelta,

    /// Tracks which chunks have already been recieved, so you can't overwrite
    /// some wrong part of a file
    #[serde(skip)]
    pub recieved_chunks: HashSet<u64>,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub offset: u64,
}
