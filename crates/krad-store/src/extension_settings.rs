use crate::config_dir::config_dir;
use crate::layout::write_atomically;
use krad_domain::ExtensionSettings;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

const EXTENSION_SETTINGS_FILE_NAME: &str = "extension-settings.json";
const NO_CONFIG_DIR: &str = "no config directory";

fn extension_settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(EXTENSION_SETTINGS_FILE_NAME))
}

#[must_use]
pub fn read_extension_settings() -> ExtensionSettings {
    extension_settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// # Errors
///
/// Fails when there is no config folder, or it cannot be written.
pub fn write_extension_settings(settings: &ExtensionSettings) -> std::io::Result<()> {
    let path =
        extension_settings_path().ok_or_else(|| Error::new(ErrorKind::NotFound, NO_CONFIG_DIR))?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;
    write_atomically(&path, &bytes)
}
