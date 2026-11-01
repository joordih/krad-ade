use crate::config_dir::config_dir;
use crate::layout::write_atomically;
use krad_domain::ArcadeScores;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

const ARCADE_FILE_NAME: &str = "arcade.json";
const NO_CONFIG_DIR: &str = "no config directory";

fn arcade_file_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(ARCADE_FILE_NAME))
}

#[must_use]
pub fn read_arcade_scores() -> ArcadeScores {
    arcade_file_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// # Errors
///
/// Fails when there is no config folder, or it cannot be written.
pub fn write_arcade_scores(scores: &ArcadeScores) -> std::io::Result<()> {
    let path = arcade_file_path().ok_or_else(|| Error::new(ErrorKind::NotFound, NO_CONFIG_DIR))?;
    let bytes = serde_json::to_vec_pretty(scores)
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;
    write_atomically(&path, &bytes)
}
