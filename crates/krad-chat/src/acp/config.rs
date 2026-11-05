use serde::{Deserialize, Serialize};

use super::ids::{ConfigOptionId, SessionId};
use super::lenient::readable_items;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigCategory {
    Mode,
    Model,
    ThoughtLevel,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigKind {
    Select,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfigOption {
    pub id: ConfigOptionId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<ConfigCategory>,
    #[serde(rename = "type")]
    pub kind: ConfigKind,
    pub current_value: String,
    #[serde(default, deserialize_with = "readable_items")]
    pub options: Vec<ConfigSelectEntry>,
}

impl SessionConfigOption {
    pub fn choices(&self) -> impl Iterator<Item = &ConfigSelectOption> {
        self.options.iter().flat_map(|entry| match entry {
            ConfigSelectEntry::Option(option) => std::slice::from_ref(option),
            ConfigSelectEntry::Group(group) => group.options.as_slice(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigSelectEntry {
    Group(ConfigSelectGroup),
    Option(ConfigSelectOption),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSelectOption {
    pub value: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSelectGroup {
    pub group: String,
    pub name: String,
    #[serde(deserialize_with = "readable_items")]
    pub options: Vec<ConfigSelectOption>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionConfigOptionRequest {
    pub session_id: SessionId,
    pub config_id: ConfigOptionId,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionConfigOptionResponse {
    #[serde(default, deserialize_with = "readable_items")]
    pub config_options: Vec<SessionConfigOption>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opencode_selects_read_with_their_category_and_grouped_options_flatten() {
        let options: Result<SetSessionConfigOptionResponse, _> = serde_json::from_value(json!({
            "configOptions": [
                {"id": "model", "name": "Model", "category": "model", "type": "select",
                 "currentValue": "opencode/big-pickle",
                 "options": [{"value": "opencode/big-pickle", "name": "opencode/Big Pickle"}]},
                {"id": "mode", "name": "Session Mode", "category": "mode", "type": "select",
                 "currentValue": "build",
                 "options": [{"group": "agents", "name": "Agents", "options": [
                     {"value": "build", "name": "Build"}, {"value": "plan", "name": "Plan"}]}]},
                {"id": "fast", "name": "Fast", "type": "boolean", "currentValue": true}
            ]
        }));
        let options = options.map_or_default(|response| response.config_options);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].category, Some(ConfigCategory::Model));
        let modes: Vec<&str> = options[1]
            .choices()
            .map(|choice| choice.value.as_str())
            .collect();
        assert_eq!(modes, ["build", "plan"]);
    }
}
