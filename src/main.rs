use std::{
    fs,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use chrono::TimeDelta;
use confetti_box::{
    database::{clean_database, Chunkbase, Mochibase},
    endpoints, pages, resources,
    settings::Settings,
};
use log::info;
use rocket::{
    data::ToByteUnit as _,
    routes,
    tokio::{self, select, sync::broadcast::Receiver, time},
};

#[rocket::main]
async fn main() {
    // Get or create config file
    let config = Settings::open(&"./settings.toml").expect("Could not open settings file");

    if !config.temp_dir.try_exists().is_ok_and(|e| e) {
        fs::create_dir_all(config.temp_dir.clone()).expect("Failed to create temp directory");
    }

    if !config.file_dir.try_exists().is_ok_and(|e| e) {
        fs::create_dir_all(config.file_dir.clone()).expect("Failed to create file directory");
    }

    // Set rocket configuration settings
    let rocket_config = rocket::Config {
        address: config.server.address.parse().expect("IP address invalid"),
        port: config.server.port,
        temp_dir: config.temp_dir.clone().into(),
        limits: rocket::data::Limits::default()
            .limit("data-form", config.max_filesize.bytes())
            .limit("file", config.max_filesize.bytes()),
        ..Default::default()
    };

    let database = Arc::new(RwLock::new(
        Mochibase::open_or_new(&config.database_path).expect("Failed to open or create database"),
    ));
    let chunkbase = Arc::new(RwLock::new(Chunkbase::default()));
    let local_db = database.clone();
    let local_chunk = chunkbase.clone();

    let (shutdown, rx) = tokio::sync::broadcast::channel(1);
    // Clean the database every 2 minutes
    tokio::spawn({
        let cleaner_db = database.clone();
        let file_path = config.file_dir.clone();
        async move { clean_loop(cleaner_db, file_path, rx).await }
    });
    tokio::spawn({
        let cleaner_db = database.clone();
        let file_path = config.file_dir.clone();
        let rx2 = shutdown.subscribe();
        async move { clean_loop(cleaner_db, file_path, rx2).await }
    });

    let rocket = rocket::build()
        .mount(
            config.server.root_path.clone() + "/",
            routes![
                confetti_box::home,
                pages::api_info,
                pages::about,
                resources::favicon,
                resources::form_handler_js,
                resources::stylesheet,
                resources::font_static,
            ],
        )
        .mount(
            config.server.root_path.clone() + "/",
            routes![
                confetti_box::chunked_upload_start,
                confetti_box::chunked_upload_continue,
                confetti_box::chunked_upload_finish,
                endpoints::server_info,
                endpoints::file_info,
                endpoints::lookup_mmid,
                endpoints::lookup_mmid_noredir,
                endpoints::lookup_mmid_name,
            ],
        )
        .manage(database)
        .manage(chunkbase)
        .manage(config)
        .configure(rocket_config)
        .launch()
        .await;

    // Ensure the server gracefully shuts down
    rocket.expect("Server failed to shutdown gracefully");

    info!("Stopping database cleaning thread...");
    shutdown.send(()).expect("Failed to stop cleaner thread.");
    info!("Stopping database cleaning thread completed successfully.");

    info!("Saving database on shutdown...");
    local_db
        .write()
        .unwrap()
        .save()
        .expect("Failed to save database");
    info!("Saving database completed successfully.");

    info!("Deleting chunk data on shutdown...");
    local_chunk
        .write()
        .unwrap()
        .delete_all()
        .expect("Failed to delete chunks");
    info!("Deleting chunk data completed successfully.");
}

/// A loop to clean the database periodically.
pub async fn clean_loop(
    main_db: Arc<RwLock<Mochibase>>,
    file_path: PathBuf,
    mut shutdown_signal: Receiver<()>,
) {
    let mut interval = time::interval(TimeDelta::minutes(2).to_std().unwrap());
    loop {
        select! {
            _ = interval.tick() => clean_database(&main_db, &file_path),
            _ = shutdown_signal.recv() => break,
        };
    }
}

pub async fn clean_chunks(chunk_db: Arc<RwLock<Chunkbase>>, mut shutdown_signal: Receiver<()>) {
    let mut interval = time::interval(TimeDelta::seconds(30).to_std().unwrap());
    loop {
        select! {
            _ = interval.tick() => {let _ = chunk_db.write().unwrap().delete_timed_out();},
            _ = shutdown_signal.recv() => break,
        };
    }
}
