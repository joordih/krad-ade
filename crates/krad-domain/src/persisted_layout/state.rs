use crate::environment::EnvironmentData;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{UnreadableEnvironment, LAYOUT_SCHEMA_VERSION};

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeState {
    pub version: u32,
    pub environments: Vec<EnvironmentData>,
    pub unreadable: Vec<UnreadableEnvironment>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            version: LAYOUT_SCHEMA_VERSION,
            environments: Vec::new(),
            unreadable: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
struct StoredState {
    version: u32,
    #[serde(default)]
    environments: Vec<serde_json::Value>,
}

impl Serialize for RuntimeState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let readable = self
            .environments
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(serde::ser::Error::custom)?;
        StoredState {
            version: self.version,
            environments: readable
                .into_iter()
                .chain(self.unreadable.iter().map(|kept| kept.0.clone()))
                .collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RuntimeState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredState::deserialize(deserializer)?;
        let mut environments = Vec::new();
        let mut unreadable = Vec::new();
        for raw in stored.environments {
            match EnvironmentData::deserialize(&raw) {
                Ok(environment) => environments.push(environment),
                Err(_) => unreadable.push(UnreadableEnvironment(raw)),
            }
        }
        Ok(Self {
            version: stored.version,
            environments,
            unreadable,
        })
    }
}
