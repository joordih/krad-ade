use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

const REDACTED: &str = "[REDACTED]";
const SECRET_KEY_SEPARATOR: char = '/';

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtensionSettingValue {
    Toggle(bool),
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtensionSettings(BTreeMap<String, BTreeMap<String, ExtensionSettingValue>>);

impl ExtensionSettings {
    #[must_use]
    pub fn of(&self, extension: &str) -> BTreeMap<String, ExtensionSettingValue> {
        self.0.get(extension).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, extension: &str, key: &str, value: ExtensionSettingValue) {
        self.0
            .entry(extension.to_owned())
            .or_default()
            .insert(key.to_owned(), value);
    }

    pub fn forget(&mut self, extension: &str) {
        self.0.remove(extension);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtensionSecretKey {
    pub extension: String,
    pub name: String,
}

impl fmt::Display for ExtensionSecretKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{SECRET_KEY_SEPARATOR}{}",
            self.extension, self.name
        )
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtensionSecret(String);

impl ExtensionSecret {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ExtensionSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_kept_per_extension_and_read_back_by_type() -> serde_json::Result<()> {
        let mut settings = ExtensionSettings::default();
        settings.set(
            "docker",
            "show-stopped",
            ExtensionSettingValue::Toggle(true),
        );
        settings.set("docker", "poll", ExtensionSettingValue::Number(2.5));
        let text = serde_json::to_string(&settings)?;
        assert_eq!(text, r#"{"docker":{"poll":2.5,"show-stopped":true}}"#);
        let back: ExtensionSettings = serde_json::from_str(&text)?;
        assert_eq!(back, settings);
        assert!(back.of("other").is_empty());
        Ok(())
    }

    #[test]
    fn a_secret_never_prints_and_its_key_names_its_extension() {
        let secret = ExtensionSecret::new("token".to_owned());
        assert_eq!(format!("{secret:?}"), REDACTED);
        let key = ExtensionSecretKey {
            extension: "docker".to_owned(),
            name: "registry-token".to_owned(),
        };
        assert_eq!(key.to_string(), "docker/registry-token");
    }
}
