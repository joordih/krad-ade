use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::channel;

use super::rpc::{AppServer, CodexError};
use crate::control::{Catalog, CommandOffer, ModelOffer};
use crate::process::Runner;

const MODEL_LIST: &str = "model/list";
const SKILLS_LIST: &str = "skills/list";
const ACCOUNT_READ: &str = "account/read";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EffortOption {
    reasoning_effort: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    id: String,
    display_name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    supported_reasoning_efforts: Vec<EffortOption>,
}

impl From<&ModelInfo> for ModelOffer {
    fn from(model: &ModelInfo) -> Self {
        Self {
            value: model.id.clone(),
            name: model
                .display_name
                .clone()
                .unwrap_or_else(|| model.id.clone()),
            description: model.description.clone(),
            efforts: model
                .supported_reasoning_efforts
                .iter()
                .filter_map(|option| option.reasoning_effort.parse().ok())
                .collect(),
            thinking_off: false,
            source: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Listed<T> {
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct SkillInfo {
    name: String,
    #[serde(default)]
    description: String,
    path: String,
    #[serde(default = "enabled")]
    enabled: bool,
}

const fn enabled() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct SkillsEntry {
    #[serde(default)]
    skills: Vec<SkillInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountInfo {
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountRead {
    account: Option<AccountInfo>,
}

#[derive(Serialize)]
struct NoParams {}

#[derive(Serialize)]
struct SkillsParams<'a> {
    cwds: [&'a str; 1],
}

#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) catalog: Catalog,
    pub(crate) skills: HashMap<String, String>,
}

fn offers(models: &[ModelInfo]) -> Vec<ModelOffer> {
    let mut shown: Vec<&ModelInfo> = models.iter().filter(|model| !model.hidden).collect();
    shown.sort_by_key(|model| !model.is_default);
    shown.into_iter().map(ModelOffer::from).collect()
}

fn models_of(server: &AppServer) -> Result<Vec<ModelOffer>, CodexError> {
    let listed: Listed<ModelInfo> = server.call(MODEL_LIST, &NoParams {})?;
    Ok(offers(&listed.data))
}

pub(crate) fn listing(server: &AppServer, directory: &str) -> Listing {
    let models = models_of(server).unwrap_or_default();
    let skills: Vec<SkillInfo> = server
        .call::<_, Listed<SkillsEntry>>(SKILLS_LIST, &SkillsParams { cwds: [directory] })
        .map_or_default(|listed| {
            listed
                .data
                .into_iter()
                .flat_map(|entry| entry.skills)
                .filter(|skill| skill.enabled)
                .collect()
        });
    let subscription = server
        .call::<_, AccountRead>(ACCOUNT_READ, &NoParams {})
        .ok()
        .and_then(|read| read.account)
        .and_then(|account| account.plan_type);
    Listing {
        catalog: Catalog {
            models,
            commands: skills
                .iter()
                .map(|skill| CommandOffer {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                })
                .collect(),
            subscription,
        },
        skills: skills
            .into_iter()
            .map(|skill| (skill.name, skill.path))
            .collect(),
    }
}

/// # Errors
///
/// Fails when codex cannot be started or cannot list its models.
pub fn codex_models(runner: &Runner, cwd: &Path) -> Result<Vec<ModelOffer>, CodexError> {
    let (pushed, _ignored) = channel();
    let server = AppServer::start(runner, cwd, &[], &[], pushed)?;
    models_of(&server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::Effort;
    use serde_json::json;

    #[test]
    fn the_default_model_comes_first_and_hidden_ones_are_left_out() {
        let models: Vec<ModelInfo> = serde_json::from_value(json!([
            {"id": "gpt-6.1-luna", "displayName": "GPT-6.1-Luna", "hidden": false, "isDefault": false,
             "supportedReasoningEfforts": [{"reasoningEffort": "low"}]},
            {"id": "gpt-6.1-sol", "displayName": "GPT-6.1-Sol", "hidden": false, "isDefault": true,
             "supportedReasoningEfforts": [{"reasoningEffort": "low"}, {"reasoningEffort": "medium"},
                                           {"reasoningEffort": "high"}, {"reasoningEffort": "xhigh"},
                                           {"reasoningEffort": "minimal"}]},
            {"id": "internal", "hidden": true}
        ]))
        .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let offered = offers(&models);
        let names: Vec<&str> = offered.iter().map(|offer| offer.name.as_str()).collect();
        assert_eq!(names, ["GPT-6.1-Sol", "GPT-6.1-Luna"]);
        assert_eq!(
            offered[0].efforts,
            [Effort::Low, Effort::Medium, Effort::High, Effort::ExtraHigh]
        );
    }
}
