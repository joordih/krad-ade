use crate::config_dir::config_dir;
use crate::layout::write_atomically;
use krad_domain::Settings;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

const SETTINGS_FILE_NAME: &str = "settings.json";
const NO_CONFIG_DIR: &str = "no config directory";

fn settings_file_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(SETTINGS_FILE_NAME))
}

#[must_use]
pub fn read_settings() -> Settings {
    settings_file_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// # Errors
///
/// Fails when there is no config folder, or it cannot be written.
pub fn write_settings(settings: &Settings) -> std::io::Result<()> {
    let path =
        settings_file_path().ok_or_else(|| Error::new(ErrorKind::NotFound, NO_CONFIG_DIR))?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;
    write_atomically(&path, &bytes)
}
