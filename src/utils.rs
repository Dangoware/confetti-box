use std::path::Path;
use blake3::Hash;

/// Get a filename based on the file's hashed name
pub fn get_id(name: &str, hash: Hash) -> String {
    hash.to_hex()[0..10].to_string() + "_" + name
}

/// Get the Blake3 hash of a file, without reading it all into memory, and also get the size
pub async fn hash_file<P: AsRef<Path>>(input: &P) -> Result<Hash, std::io::Error> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap_rayon(input)?;

    Ok(hasher.finalize())
}
