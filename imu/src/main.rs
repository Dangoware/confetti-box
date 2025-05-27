use std::{fs, io::{Read, Write}, path::{Path, PathBuf}};

use base64::{prelude::BASE64_URL_SAFE, Engine};
use chrono::{DateTime, Datelike, Local, Month, TimeDelta, Timelike, Utc};

use confetti_box::{database::MochiFile, endpoints::ServerInfo, strings::{parse_time_string, pretty_time, pretty_time_short, BreakStyle, TimeGranularity}};
use futures_util::{stream::FusedStream as _, SinkExt as _, StreamExt as _};
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use reqwest::{header::HeaderValue, Client};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs::File, io::{AsyncReadExt, AsyncWriteExt}, join};
use tokio_tungstenite::{connect_async, tungstenite::{client::IntoClientRequest as _, Message}};
use url::Url;
use clap::{arg, builder::{styling::RgbColor, Styles}, Parser, Subcommand};
use anyhow::{anyhow, bail, Context as _, Result};

const CLAP_STYLE: Styles = Styles::styled()
    .header(RgbColor::on_default(RgbColor(197,229,207)).italic())
    .usage(RgbColor::on_default(RgbColor(174,196,223)))
    .literal(RgbColor::on_default(RgbColor(246,199,219)))
    .placeholder(RgbColor::on_default(RgbColor(117,182,194)))
    .error(RgbColor::on_default(RgbColor(181,66,127)).underline());

const DEBUG_CONFIG: &str = "test/config.toml";
const DEBUG_DOWNLOAD_DIR: &str = "test/downloads/";

#[derive(Parser)]
#[command(name = "confetti_cli")]
#[command(version, about, long_about = None)]
#[command(styles = CLAP_STYLE)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Upload files
    #[command(visible_alias="u")]
    Upload {
        /// Filename(s) to upload
        #[arg(value_name = "file(s)", required = true)]
        files: Vec<PathBuf>,

        /// Expiration length of the uploaded file
        #[arg(short, long, default_value = "6h")]
        duration: String,
    },

    /// Set config options
    Set {
        /// Set the username for a server which requires login
        #[arg(short, long, required = false)]
        username: Option<String>,
        /// Set the password for a server which requires login
        #[arg(short, long, required = false)]
        password: Option<String>,
        /// Set the URL of the server to connect to (assumes https://)
        #[arg(long, required = false)]
        url: Option<String>,
        /// Set the directory to download into by default
        #[arg(value_name="directory", short_alias='d', long, required = false)]
        dl_dir: Option<String>,
    },

    /// Get server information manually
    Info,

    /// Download files
    #[command(visible_alias="d")]
    Download {
        /// MMID to download
        #[arg(value_name = "mmid(s)", required = true)]
        mmids: Vec<String>,
        #[arg(short, long, value_name = "out", required = false)]
        out_directory: Option<PathBuf>
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::open().unwrap();

    match &cli.command {
        Commands::Upload { files, duration } => {
            let Some(url) = config.url.clone() else {
                exit_error(
                    "URL is empty",
                    Some(&format!("Please set it using the {} command", "set".truecolor(246,199,219).bold())),
                    None,
                );
            };

            get_info_if_expired(&mut config).await?;

            let duration = match parse_time_string(duration) {
                Ok(d) => d,
                Err(e) => return Err(anyhow!("Invalid duration: {e}")),
            };

            if !config.info.as_ref().unwrap().allowed_durations.contains(&(duration.num_seconds() as u32)) {
                let pretty_durations: Vec<String> = config.info.as_ref()
                    .unwrap()
                    .allowed_durations
                    .clone()
                    .iter()
                    .map(|d| pretty_time_short(*d as i64))
                    .collect();

                exit_error(
                    "Duration not allowed.",
                    Some("Please choose from:"),
                    Some(pretty_durations)
                );
            }

            println!("Uploading...");
            for path in files {
                if !path.try_exists().is_ok_and(|t| t) {
                    print_error_line(&format!("The file {:#?} does not exist", path.truecolor(234, 129, 100)));
                    continue;
                }

                let name = path.file_name().unwrap().to_string_lossy();
                let response = upload_file(
                    name.into_owned(),
                    &path,
                    &url,
                    duration,
                    &config.login
                ).await.with_context(|| "Failed to upload").unwrap();

                let datetime: DateTime<Local> = DateTime::from(response.expiry());
                let date = format!(
                    "{} {}",
                    Month::try_from(u8::try_from(datetime.month()).unwrap()).unwrap().name(),
                    datetime.day(),
                );
                let time = format!("{:02}:{:02}", datetime.hour(), datetime.minute());
                println!(
                    "{:>8} {}, {} (in {})\n{:>8} {}",
                    "Expires:".truecolor(174,196,223).bold(),
                    date,
                    time,
                    pretty_time(duration.num_seconds(), BreakStyle::Space, TimeGranularity::Seconds),
                    "URL:".truecolor(174,196,223).bold(), (url.to_string() + "f/" + &response.mmid().to_string()).underline()
                );
            }
        }
        Commands::Download { mmids, out_directory } => {
            let Some(url) = config.url else {
                exit_error(
                    "URL is empty",
                    Some(&format!("Please set it using the {} command", "set".truecolor(246,199,219).bold())),
                    None,
                );
            };

            let out_directory = if let Some(dir) = out_directory {
                dir
            } else {
                let ddir = &config.download_directory;
                if ddir.as_os_str().is_empty() {
                    exit_error(
                        "Default download directory is empty",
                        Some(&format!("Please set it using the {} command", "set".truecolor(246,199,219).bold())),
                        None,
                    );
                } else if !ddir.exists() {
                    exit_error(
                        &format!("Default download directory {} does not exist", ddir.display()),
                        Some(&format!("Please set it using the {} command", "set".truecolor(246,199,219).bold())),
                        None,
                        )
                } else {
                    ddir
                }
            };

            for mmid in mmids {
                let mmid = if mmid.len() != 8 {
                    if mmid.contains(format!("{url}/f/").as_str()) {
                        let mmid = mmid.replace(format!("{url}/f/").as_str(), "");
                        if mmid.len() != 8 {
                            exit_error("{mmid} is not a valid MMID", Some("MMID must be 8 characters long"), None)
                        } else {
                            mmid
                        }
                    } else {
                        exit_error("{mmid} is not a valid MMID", Some("MMID must be 8 characters long"), None)
                    }
                } else {
                    unimplemented!();
                };

                let client = Client::new();

                let info = if let Ok(file) = if let Some(login) = &config.login {
                    client.get(format!("{}/info/{mmid}", url))
                    .basic_auth(&login.user, Some(&login.pass))
                } else {
                    client.get(format!("{}/info/{mmid}", url))
                }
                .send()
                .await
                .unwrap()
                .json::<MochiFile>()
                .await {
                    file
                } else {
                    exit_error("File with MMID {mmid} was not found", None, None)
                };

                let mut file_res = if let Some(login) = &config.login {
                    client.get(format!("{}/f/{mmid}", url))
                    .basic_auth(&login.user, Some(&login.pass))
                } else {
                    client.get(format!("{}/f/{mmid}", url))
                }
                .send()
                .await
                .unwrap();

                let out_directory = out_directory.join(info.name());
                let mut out_file: File = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .read(true)
                    .open(&out_directory).await
                    .unwrap();

                let progress_bar = ProgressBar::new(100);

                progress_bar.set_style(ProgressStyle::with_template(
                    &format!("{} {} {{bar:40.cyan/blue}} {{pos:>3}}% {{msg}}","Saving".bold(), &out_directory.file_name().unwrap().to_string_lossy().truecolor(246,199,219))
                ).unwrap());

                let mut chunk_size = 0;
                let file_size = file_res.content_length().unwrap();
                let mut first = true;

                let mut i = 0;
                while let Some(next) = file_res.chunk().await.unwrap() {
                    i+=1;
                    if first {
                        chunk_size = next.len() as u64;
                        first = false
                    }
                    out_file.write_all(&next).await.unwrap();

                    progress_bar.set_position(f64::trunc(((i as f64 * chunk_size as f64) / file_size as f64) * 200.0) as u64);
                }
                progress_bar.finish_and_clear();

                println!("Downloaded to \"{}\"", out_directory.display());
            }
        }
        Commands::Set {
            username,
            password,
            url,
            dl_dir
        } => {
            if username.is_none() && password.is_none() && url.is_none() && dl_dir.is_none() {
                exit_error(
                    "Please provide an option to set",
                    Some("Allowed options:"),
                    Some(vec!["--username".to_string(), "--password".to_string(), "--url".to_string(), "--dl-dir".to_string()]),
                );
            }

            if let Some(u) = username {
                if u.is_empty() {
                    exit_error("Username cannot be blank!", None, None);
                }

                if let Some(l) = config.login.as_mut() {
                    l.user = u.clone();
                } else {
                    config.login = Login {
                        user: u.clone(),
                        pass: "".into()
                    }.into();
                }

                config.save().unwrap();
                println!("Username set to \"{u}\"")
            }
            if let Some(p) = password {
                if p.is_empty() {
                    exit_error("Password cannot be blank", None, None);
                }

                if let Some(l) = config.login.as_mut() {
                    l.pass = p.clone();
                } else {
                    config.login = Login {
                        user: "".into(),
                        pass: p.clone()
                    }.into();
                }

                config.save().unwrap();
                println!("Password set")
            }
            if let Some(url) = url {
                if url.is_empty() {
                    exit_error("URL cannot be blank", None, None);
                }

                let url = if url.ends_with('/') {
                    url.split_at(url.len() - 1).0
                } else {
                    url
                };

                let new_url = if !url.starts_with("https://") && !url.starts_with("http://") {
                    ("https://".to_owned() + url).to_string()
                } else {
                    url.to_string()
                };

                config.url = Some(Url::parse(&new_url)?);

                config.save().unwrap();
                println!("URL set to \"{url}\"");
            }
            if let Some(mut dir) = dl_dir.clone() {
                if dir.is_empty() {
                    exit_error("Download directory cannot be blank", None, None);
                }
                if dir.as_str() == "default" {
                    dir = directories::UserDirs::new()
                    .unwrap()
                    .download_dir()
                    .unwrap_or_else(|| exit_error("No Default directory available", None, None))
                    .to_string_lossy()
                    .to_string();
                }
                if dir.ends_with('/') {
                    dir.push('/');
                }

                let _dir = PathBuf::from(dir.clone());
                if !_dir.exists() {
                    exit_error("Directory {dir} does not exist", None, None)
                }

                config.download_directory = _dir;
                config.save().unwrap();
                println!("Download directory set to \"{dir}\"");
            }
        }
        Commands::Info => {
            let info = match get_info(&config).await {
                Ok(i) => i,
                Err(e) => exit_error("Failed to get server information!", Some(e.to_string().as_str()), None),
            };
            config.info = Some(info);
            config.save().unwrap();
        }
    }

    Ok(())
}

#[derive(Error, Debug)]
enum UploadError {
    #[error("request provided was invalid: {0}")]
    WebSocketFailed(String),

    #[error("error on reqwest transaction: {0}")]
    Reqwest(#[from] reqwest::Error),
}

async fn upload_file<P: AsRef<Path>>(
    name: String,
    path: &P,
    url: &Url,
    duration: TimeDelta,
    login: &Option<Login>,
) -> Result<MochiFile, UploadError> {
    let mut file = File::open(path).await.unwrap();
    let file_size = file.metadata().await.unwrap().len();

    // Construct the URL
    let mut url = url.clone();
    if url.scheme() == "http" {
        url.set_scheme("ws").unwrap();
    } else if url.scheme() == "https" {
        url.set_scheme("wss").unwrap();
    }

    url.set_path("/upload/websocket");
    url.set_query(Some(&format!("name={}&size={}&duration={}", name, file_size, duration.num_seconds())));

    let mut request = url.to_string().into_client_request().unwrap();

    if let Some(l) = login {
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(
               &("Basic ".to_string() + &BASE64_URL_SAFE.encode(l.user.to_string() + ":" + &l.pass))
            ).unwrap()
        );
    }

    let (stream, _response) = connect_async(request).await.map_err(|e| UploadError::WebSocketFailed(e.to_string()))?;
    let (mut write, mut read) = stream.split();

    // Upload the file in chunks
    let upload_task = async move {
        let mut chunk = vec![0u8; 200_000];
        loop {
            let read_len = file.read(&mut chunk).await.unwrap();
            if read_len == 0 {
                break
            }

            write.send(Message::binary(chunk[..read_len].to_vec())).await.unwrap();
        }

        // Close the stream because sending is over
        write.send(Message::binary(b"".as_slice())).await.unwrap();
        write.flush().await.unwrap();

        write
    };

    let bar = ProgressBar::new(100);
    bar.set_style(ProgressStyle::with_template(
        &format!("{} {{bar:40.cyan/blue}} {{pos:>3}}% {{msg}}", name)
    ).unwrap());

    // Get the progress of the file upload
    let progress_task = async move {
        let final_json = loop {
            let Some(p) = read.next().await else {
                break String::new()
            };

            let p = p.unwrap();

            // Got the final json information, return that
            if p.is_text() {
                break p.into_text().unwrap().to_string()
            }

            // Get the progress information
            let prog = p.into_data();
            let prog = u64::from_le_bytes(prog.to_vec().try_into().unwrap());
            let percent = f64::trunc((prog as f64 / file_size as f64) * 100.0);
            if percent <= 100. {
                bar.set_position(percent as u64);
            }
        };

        (read, final_json, bar)
    };

    // Wait for both of the tasks to finish
    let (read, write) = join!(progress_task, upload_task);
    let (read, final_json, bar) = read;
    let mut stream = write.reunite(read).unwrap();

    let file_info: MochiFile = serde_json::from_str(&final_json).unwrap();

    // If the websocket isn't closed, do that
    if !stream.is_terminated() {
        stream.close(None).await.unwrap();
    }

    bar.finish_and_clear();

    Ok(file_info)
}

async fn get_info_if_expired(config: &mut Config) -> Result<()> {
    let now = Utc::now();
    if config.info_fetch.is_some() && config.info_fetch.is_none_or(|e| e > now) {
        // Not yet ready to get a new batch of info
        return Ok(())
    }
    println!("{}", "Getting new server info...".truecolor(255,249,184));

    let info = get_info(config).await?;
    config.info = Some(info);
    config.info_fetch = Some(now + TimeDelta::days(2));
    config.save().unwrap();

    Ok(())
}

async fn get_info(config: &Config) -> Result<ServerInfo> {
    let Some(url) = config.url.clone() else {
        exit_error(
            "URL is empty",
            Some(&format!("Please set it using the {} command", "set".truecolor(246,199,219).bold())),
            None,
        );
    };
    let client = Client::new();

    let get_info = client.get(format!("{url}/info"));
    let get_info = if let Some(l) = &config.login {
        get_info.basic_auth(&l.user, l.pass.clone().into())
    } else {
        get_info
    };

    let info = get_info.send().await.unwrap();
    if info.status() == 401 {
        let err = info.error_for_status().unwrap_err();
        bail!(
            "Got access denied! Maybe you need a username and password? ({} - {})",
            err.status().unwrap().as_str(),
            err.status().unwrap().canonical_reason().unwrap_or_default()
        )
    }
    let info = match info.error_for_status() {
        Ok(i) => i.json::<ServerInfo>().await?,
        Err(e) => bail!(
            "Network error: ({} - {})",
            e.status().unwrap().as_str(),
            e.status().unwrap().canonical_reason().unwrap_or_default()
        ),
    };

    Ok(info)
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Login {
    user: String,
    pass: String
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default)]
struct Config {
    url: Option<Url>,
    login: Option<Login>,
    /// The time when the info was last fetched
    info_fetch: Option<DateTime<Utc>>,
    info: Option<ServerInfo>,
    download_directory: PathBuf,
}

impl Config {
    fn open() -> Result<Self, Box<dyn std::error::Error>> {
        let c = if cfg!(debug_assertions) {
            if let Ok(str) = fs::read_to_string(DEBUG_CONFIG) {
                str
            } else {
                let c = Config {
                    url: None,
                    login: None,
                    info_fetch: None,
                    info: None,
                    download_directory: PathBuf::from(DEBUG_DOWNLOAD_DIR)
                };
                c.save().unwrap();
                return Ok(c);
            }
        } else if let Some(dir) = directories::ProjectDirs::from("", "Dangoware", "confetti_cli") {
            let path = dir.config_dir();
            fs::create_dir(path).or_else(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(err)
                }
            })?;

            let mut buf: String = String::new();

            fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .read(true)
                .open(path.join("config.toml"))
                .unwrap()
                .read_to_string(&mut buf)
                .unwrap();

            if buf.is_empty() {
                let c = Config {
                    url: None,
                    login: None,
                    info: None,
                    info_fetch: None,
                    download_directory: PathBuf::from(directories::UserDirs::new().unwrap().download_dir().unwrap_or(Path::new("")))
                };
                c.save().unwrap();

                // dbg!(path);
                return Ok(c);
            } else {
                buf
            }
        } else {
            panic!("no project dir?")
        };

        Ok(toml::from_str::<Config>(c.as_str()).unwrap())
    }

    fn save(&self) -> Result<(), ()> {
        let path = if cfg!(debug_assertions) {
            DEBUG_CONFIG.to_string()
        } else if let Some(dir) = directories::ProjectDirs::from("", "Dangoware", "confetti_cli") {
            let path = dir.config_dir();
            fs::create_dir(path).or_else(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(err)
                }
            }).unwrap();
            let x = path.join("config.toml");
            x.clone().to_str().unwrap().to_string()
        } else {
            panic!("no project dir?")
        };

        fs::OpenOptions::new().create(true).write(true).truncate(true).open(path).unwrap().write_all(toml::to_string(self).unwrap().as_bytes()).unwrap();
        Ok(())
    }
}

fn exit_error(main_message: &str, fix: Option<&str>, fix_values: Option<Vec<String>>) -> ! {
    print_error_line(main_message);

    if let Some(f) = fix {
        eprint!("{f} ");
        if let Some(v) = fix_values {
            let len = v.len() - 1;
            for (i, value) in v.iter().enumerate() {
                eprint!("{}", value.truecolor(234, 129, 100));
                if i != len {
                    eprint!(", ");
                }
            }
        }
        eprintln!("\n");
    }

    eprintln!("For more information, try '{}'", "--help".truecolor(246,199,219));
    std::process::exit(1)
}

fn print_error_line(message: &str) {
    eprintln!("{}: {message}", "Error".truecolor(181,66,127).italic().underline());
}
