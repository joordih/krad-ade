use crate::config_dir::config_dir;
use krad_domain::Settings;
use std::path::{Path, PathBuf};

const SETTINGS_FILE_NAME: &str = "settings.json";

fn settings_file_path() -> Option<PathBuf> {
    config_dir().map(|dir| settings_file_in(&dir))
}

#[must_use]
pub fn settings_file_in(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE_NAME)
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
