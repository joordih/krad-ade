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
        .map_or_else(Settings::default, |bytes| settings_from(&bytes))
}

fn settings_from(bytes: &[u8]) -> Settings {
    serde_json::from_slice(bytes).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AppearanceMode, Switch};

    #[test]
    fn a_bad_attention_value_keeps_every_other_setting() {
        let read = settings_from(
            br#"{
                "appearance": "Dark",
                "attention": {
                    "notifications": "off",
                    "quiet_hours": {"switch": "on", "start": "25:00"},
                    "rules": {"finished": {"sound": "kazoo"}}
                }
            }"#,
        );
        assert_eq!(read.appearance, AppearanceMode::Dark);
        assert_eq!(read.attention.notifications, Switch::Off);
        assert_eq!(read.attention.quiet_hours.switch, Switch::On);
        let whole = settings_from(br#"{"appearance": "Dark", "attention": 7}"#);
        assert_eq!(whole.appearance, AppearanceMode::Dark);
        assert!(whole.attention.is_default());
    }
}
