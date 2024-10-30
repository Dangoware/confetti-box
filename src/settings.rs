use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use chrono::TimeDelta;
use rocket::data::ToByteUnit;
use rocket::serde::{Deserialize, Serialize};
use serde_with::serde_as;

/// A response to the client from the server
#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "rocket::serde")]
pub struct Settings {
    /// Maximum filesize in bytes
    #[serde(default)]
    pub max_filesize: u64,

    /// Is overwiting already uploaded files with the same hash allowed, or is
    /// this a no-op?
    #[serde(default)]
    pub overwrite: bool,

    /// Settings pertaining to duration information
    pub duration: DurationSettings,

    /// The path to the database file
    #[serde(default)]
    pub database_path: PathBuf,

    /// Temporary directory for stuff
    #[serde(default)]
    pub temp_dir: PathBuf,

    /// Directory in which to store hosted files
    #[serde(default)]
    pub file_dir: PathBuf,

    /// Settings pertaining to the server configuration
    #[serde(default)]
    pub server: ServerSettings,

    #[serde(skip)]
    path: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_filesize: 1.megabytes().into(), // 1 MB
            overwrite: true,
            duration: DurationSettings::default(),
            server: ServerSettings::default(),
            path: "./settings.toml".into(),
            database_path: "./database.mochi".into(),
            temp_dir: std::env::temp_dir(),
            file_dir: "./files/".into(),
        }
    }
}

impl Settings {
    pub fn open<P: AsRef<Path>>(path: &P) -> Result<Self, io::Error> {
        let mut input_str = String::new();
        if !path.as_ref().exists() {
            let new_self = Self {
                path: path.as_ref().to_path_buf(),
                ..Default::default()
            };
            new_self.save()?;
            return Ok(new_self);
        } else {
            File::open(path).unwrap().read_to_string(&mut input_str)?;
        }

        let mut parsed_settings: Self = toml::from_str(&input_str).unwrap();
        parsed_settings.path = path.as_ref().to_path_buf();

        Ok(parsed_settings)
    }

    pub fn save(&self) -> Result<(), io::Error> {
        let mut out_path = self.path.clone();
        out_path.set_extension(".bkp");
        let mut file = File::create(&out_path).expect("Could not save!");
        file.write_all(&toml::to_string_pretty(self).unwrap().into_bytes())?;

        fs::rename(out_path, &self.path).unwrap();

        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "rocket::serde")]
pub struct ServerSettings {
    pub domain: String,
    pub address: String,
    pub port: u16,

    /// The path to the root directory of the program, ex `/filehost/`
    pub root_path: String,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            domain: "example.com".into(),
            address: "127.0.0.1".into(),
            root_path: "/".into(),
            port: 8950,
        }
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug)]
pub struct DurationSettings {
    /// Maximum file lifetime, seconds
    #[serde(default)]
    #[serde_as(as = "serde_with::DurationSeconds<i64>")]
    pub maximum: TimeDelta,

    /// Default file lifetime, seconds
    #[serde(default)]
    #[serde_as(as = "serde_with::DurationSeconds<i64>")]
    pub default: TimeDelta,

    /// List of recommended lifetimes
    #[serde(default)]
    #[serde_as(as = "Vec<serde_with::DurationSeconds<i64>>")]
    pub allowed: Vec<TimeDelta>,

    /// Restrict the input durations to the allowed ones or not
    #[serde(default)]
    pub restrict_to_allowed: bool,
}

impl Default for DurationSettings {
    fn default() -> Self {
        Self {
            maximum: TimeDelta::days(3),  // 72 hours
            default: TimeDelta::hours(6), // 6 hours
            // 1 hour, 6 hours, 24 hours, and 48 hours
            allowed: vec![
                TimeDelta::hours(1),
                TimeDelta::hours(6),
                TimeDelta::days(1),
                TimeDelta::days(2),
            ],
            restrict_to_allowed: true,
        }
    }
}
