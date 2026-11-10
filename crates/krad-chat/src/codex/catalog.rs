use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::channel;

use super::limits::{LimitsRead, ReadParams, RATE_LIMITS_READ};
use super::rpc::{AppServer, CodexError};
use crate::control::{Catalog, CommandOffer, ModelOffer, ModelSource};
use crate::process::Runner;
use krad_domain::{humanize_model_id, split_context, Effort};

const MODEL_LIST: &str = "model/list";
const SKILLS_LIST: &str = "skills/list";
const ACCOUNT_READ: &str = "account/read";
const OPENAI: &str = "OpenAI";

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
    default_reasoning_effort: Option<String>,
}

impl From<&ModelInfo> for ModelOffer {
    fn from(model: &ModelInfo) -> Self {
        Self {
            value: model.id.clone(),
            name: model
                .display_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| humanize_model_id(&model.id))
                .unwrap_or_else(|| model.id.clone()),
            description: model.description.clone(),
            efforts: Effort::ordered(
                model
                    .supported_reasoning_efforts
                    .iter()
                    .filter_map(|option| option.reasoning_effort.parse().ok()),
            ),
            default_effort: model
                .default_reasoning_effort
                .as_deref()
                .and_then(|level| level.parse().ok()),
            source: Some(ModelSource::Provider(OPENAI.to_owned())),
            context: split_context(&model.id).1,
            recommended: model.is_default,
            ..Self::default()
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
    pub(crate) limits: Option<LimitsRead>,
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
    let limits = server
        .call::<_, LimitsRead>(RATE_LIMITS_READ, &ReadParams::default())
        .ok();
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
        limits,
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
            [
                Effort::Minimal,
                Effort::Low,
                Effort::Medium,
                Effort::High,
                Effort::ExtraHigh
            ]
        );
        assert!(offered[0].recommended);
        assert!(!offered[1].recommended);
    }

    #[test]
    fn levels_come_in_order_with_ultra_and_the_default_level() {
        let models: Vec<ModelInfo> = serde_json::from_value(json!([
            {"id": "gpt-6.1-sol", "displayName": "GPT-6.1-Sol", "isDefault": true,
             "defaultReasoningEffort": "low",
             "supportedReasoningEfforts": [{"reasoningEffort": "ultra"}, {"reasoningEffort": "low"},
                                           {"reasoningEffort": "max"}, {"reasoningEffort": "none"}]},
            {"id": "gpt-7-nova"}
        ]))
        .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let offered = offers(&models);
        assert_eq!(
            offered[0].efforts,
            [Effort::Low, Effort::Max, Effort::Ultra]
        );
        assert_eq!(offered[0].default_effort, Some(Effort::Low));
        assert_eq!(
            offered[0].source,
            Some(ModelSource::Provider(OPENAI.to_owned()))
        );
        assert_eq!(offered[1].name, "GPT-7 Nova");
        assert_eq!(offered[1].efforts, []);
    }
}
