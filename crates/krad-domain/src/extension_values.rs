use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::extension_id::ExtensionId;

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
pub struct ExtensionSettings(BTreeMap<ExtensionId, BTreeMap<String, ExtensionSettingValue>>);

impl ExtensionSettings {
    #[must_use]
    pub fn of(&self, extension: &ExtensionId) -> BTreeMap<String, ExtensionSettingValue> {
        self.0.get(extension).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, extension: &ExtensionId, key: &str, value: ExtensionSettingValue) {
        self.0
            .entry(extension.clone())
            .or_default()
            .insert(key.to_owned(), value);
    }

    pub fn forget(&mut self, extension: &ExtensionId) {
        self.0.remove(extension);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtensionSecretKey {
    pub extension: ExtensionId,
    pub name: String,
}

impl ExtensionSecretKey {
    #[must_use]
    pub fn new(extension: &ExtensionId, name: &str) -> Self {
        Self {
            extension: extension.clone(),
            name: name.to_owned(),
        }
    }
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn values_are_kept_per_extension_and_read_back_by_type() -> TestResult {
        let docker: ExtensionId = "docker".parse()?;
        let mut settings = ExtensionSettings::default();
        settings.set(&docker, "show-stopped", ExtensionSettingValue::Toggle(true));
        settings.set(&docker, "poll", ExtensionSettingValue::Number(2.5));
        let text = serde_json::to_string(&settings)?;
        assert_eq!(text, r#"{"docker":{"poll":2.5,"show-stopped":true}}"#);
        let back: ExtensionSettings = serde_json::from_str(&text)?;
        assert_eq!(back, settings);
        assert!(back.of(&"other".parse()?).is_empty());
        let mut forgotten = back;
        forgotten.forget(&docker);
        assert!(forgotten.of(&docker).is_empty());
        Ok(())
    }

    #[test]
    fn a_secret_never_prints_and_its_key_names_its_extension() -> TestResult {
        let secret = ExtensionSecret::new("token".to_owned());
        assert_eq!(format!("{secret:?}"), REDACTED);
        let key = ExtensionSecretKey::new(&"docker".parse()?, "registry-token");
        assert_eq!(key.to_string(), "docker/registry-token");
        Ok(())
    }
}
