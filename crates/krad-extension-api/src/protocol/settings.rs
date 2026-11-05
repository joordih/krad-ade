use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::id::{SecretName, SettingKey};
use crate::protocol::{notification, request, Empty};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SettingValue {
    Toggle(bool),
    Number(f64),
    Text(String),
}

pub type SettingsValues = BTreeMap<SettingKey, SettingValue>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetSecretParams {
    pub name: SecretName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetSecretParams {
    pub name: SecretName,
    #[serde(default)]
    pub value: Option<Secret>,
}

request!(GetSettings, "settings/get", App, Empty, SettingsValues);
notification!(SettingsChanged, "settings/changed", Program, SettingsValues);
request!(
    GetSecret,
    "secret/get",
    App,
    GetSecretParams,
    Option<Secret>
);
request!(SetSecret, "secret/set", App, SetSecretParams, Empty);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints_its_value() {
        let secret = Secret::new("hunter2".to_owned());
        assert_eq!(format!("{secret:?}"), "[REDACTED]");
        assert_eq!(secret.expose(), "hunter2");
    }
}
