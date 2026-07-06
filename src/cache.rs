use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::error::{CliarrError, Result};

pub fn poster_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "cliarr")
        .ok_or_else(|| CliarrError::Config("cannot determine cache directory".into()))?;
    let dir = dirs.cache_dir().join("posters");
    std::fs::create_dir_all(&dir)
        .map_err(|e| CliarrError::Config(format!("cannot create {}: {e}", dir.display())))?;
    Ok(dir)
}

pub fn poster_path(url: &str) -> Result<PathBuf> {
    let hash = Sha256::digest(url.as_bytes());
    Ok(poster_dir()?.join(format!("{:x}.img", hash)))
}
