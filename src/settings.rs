use std::{fs::{self, File}, io::{self, Read, Write}, path::{Path, PathBuf}};

use rocket::serde::{Deserialize, Serialize};

/// A response to the client from the server
#[derive(Deserialize, Serialize, Debug)]
#[serde(crate = "rocket::serde")]
pub struct Settings {
    /// Maximum filesize in bytes
    pub max_filesize: u64,

    /// Maximum file lifetime, seconds
    pub max_duration: u32,

    /// The path to the root directory of the program, ex `/filehost/`
    pub root_path: String,

    /// The path to the database file
    pub database_path: PathBuf,

    /// Temporary directory for stuff
    pub temp_dir: PathBuf,

    pub server: ServerSettings,

    #[serde(skip)]
    path: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_filesize: 128_000_000, // 128MB
            max_duration: 86_400, // 1 day
            root_path: "/".into(),
            server: ServerSettings::default(),
            path: "./settings.toml".into(),
            database_path: "./database.mochi".into(),
            temp_dir: std::env::temp_dir()
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
    pub address: String,
    pub port: u16,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".into(),
            port: 8955
        }
    }
}
