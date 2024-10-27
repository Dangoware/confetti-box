use std::{path::PathBuf, sync::{Arc, RwLock}};
use rocket::{fs::NamedFile, get, State};

use crate::database::Database;

#[get("/<ident..>")]
async fn files(
    db: &State<Arc<RwLock<Database>>>,
    ident: PathBuf
) -> Option<NamedFile> {
    //let file = NamedFile::open(Path::new("static/").join(file)).await.ok();

    todo!()
}
