use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::thread;

use super::server::{OpencodeError, OpencodeServer};
use crate::control::{Catalog, CommandOffer, ModelOffer, ModelSource};

const MODELS_PATH: &str = "/api/model";
const DEFAULT_MODEL_PATH: &str = "/api/model/default";
const COMMANDS_PATH: &str = "/api/command";
const SKILLS_PATH: &str = "/api/skill";
const PROVIDERS_PATH: &str = "/api/provider";
const DEPRECATED: &str = "deprecated";
const GO_PROVIDER: &str = "opencode-go";
const ZEN_PROVIDER: &str = "opencode";

#[derive(Debug, Deserialize)]
struct Variant {
    id: String,
}

#[derive(Debug, Default, Deserialize)]
struct Limit {
    #[serde(default)]
    context: u64,
}

#[derive(Debug, Default, Deserialize)]
struct Price {
    #[serde(default)]
    input: f64,
    #[serde(default)]
    output: f64,
}

#[derive(Debug, Deserialize)]
struct ProviderInfo {
    id: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
    #[serde(rename = "providerID")]
    provider_id: String,
    name: Option<String>,
    #[serde(default)]
    variants: Vec<Variant>,
    #[serde(default)]
    limit: Limit,
    #[serde(default = "enabled")]
    enabled: bool,
    status: Option<String>,
    #[serde(default)]
    cost: Vec<Price>,
}

const fn enabled() -> bool {
    true
}

impl ModelInfo {
    fn selector(&self) -> String {
        format!("{}/{}", self.provider_id, self.id)
    }

    fn usable(&self) -> bool {
        self.enabled && self.status.as_deref() != Some(DEPRECATED)
    }

    fn free(&self) -> bool {
        !self.cost.is_empty()
            && self
                .cost
                .iter()
                .all(|price| price.input == 0.0 && price.output == 0.0)
    }

    fn source(&self, providers: &HashMap<String, String>) -> ModelSource {
        if self.free() {
            return ModelSource::Free;
        }
        match self.provider_id.as_str() {
            GO_PROVIDER => ModelSource::OpencodeGo,
            ZEN_PROVIDER => ModelSource::OpencodeZen,
            other => ModelSource::Provider(
                providers
                    .get(other)
                    .cloned()
                    .unwrap_or_else(|| other.to_owned()),
            ),
        }
    }

    fn offer(&self, providers: &HashMap<String, String>) -> ModelOffer {
        ModelOffer {
            source: Some(self.source(providers)),
            ..ModelOffer::from(self)
        }
    }
}

impl From<&ModelInfo> for ModelOffer {
    fn from(model: &ModelInfo) -> Self {
        Self {
            value: model.selector(),
            name: model.name.clone().unwrap_or_else(|| model.id.clone()),
            description: Some(model.provider_id.clone()),
            efforts: model
                .variants
                .iter()
                .filter_map(|variant| variant.id.parse().ok())
                .collect(),
            thinking_off: false,
            source: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CommandInfo {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkillInfo {
    id: String,
    description: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) catalog: Catalog,
    pub(crate) windows: HashMap<String, u64>,
    pub(crate) commands: HashSet<String>,
    pub(crate) skills: HashSet<String>,
}

fn provider_names(providers: Vec<ProviderInfo>) -> HashMap<String, String> {
    providers
        .into_iter()
        .filter_map(|provider| Some((provider.id, provider.name?)))
        .collect()
}

fn offers(
    models: &[ModelInfo],
    default: Option<&ModelInfo>,
    providers: &HashMap<String, String>,
) -> Vec<ModelOffer> {
    let first = default.map(ModelInfo::selector);
    let free_on_zen: HashSet<&str> = models
        .iter()
        .filter(|model| model.usable() && model.free() && model.provider_id == ZEN_PROVIDER)
        .map(|model| model.id.as_str())
        .collect();
    let mut offers: Vec<ModelOffer> = models
        .iter()
        .filter(|model| model.usable())
        .filter(|model| {
            !model.free()
                || model.provider_id == ZEN_PROVIDER
                || !free_on_zen.contains(model.id.as_str())
        })
        .map(|model| model.offer(providers))
        .collect();
    if let Some(at) = first.and_then(|first| offers.iter().position(|offer| offer.value == first)) {
        offers[..=at].rotate_right(1);
    }
    offers
}

/// # Errors
///
/// Fails when the server cannot list its models.
pub fn opencode_models(
    server: &OpencodeServer,
    directory: Option<&str>,
) -> Result<Vec<ModelOffer>, OpencodeError> {
    let (models, default, providers) = thread::scope(|scope| {
        let models = scope.spawn(|| server.get::<Vec<ModelInfo>>(MODELS_PATH, directory));
        let default =
            scope.spawn(|| server.get::<Option<ModelInfo>>(DEFAULT_MODEL_PATH, directory));
        let providers = server.get::<Vec<ProviderInfo>>(PROVIDERS_PATH, directory);
        (models.join(), default.join(), providers)
    });
    let models = models.unwrap_or_else(|_| Ok(Vec::new()))?;
    let default = default.ok().and_then(Result::ok).flatten();
    let providers = provider_names(providers.unwrap_or_default());
    Ok(offers(&models, default.as_ref(), &providers))
}

pub(crate) fn listing(server: &OpencodeServer, directory: &str) -> Listing {
    let directory = Some(directory);
    let (models, default, commands, providers, skills) = thread::scope(|scope| {
        let models = scope.spawn(|| server.get::<Vec<ModelInfo>>(MODELS_PATH, directory));
        let default =
            scope.spawn(|| server.get::<Option<ModelInfo>>(DEFAULT_MODEL_PATH, directory));
        let commands = scope.spawn(|| server.get::<Vec<CommandInfo>>(COMMANDS_PATH, directory));
        let providers = scope.spawn(|| server.get::<Vec<ProviderInfo>>(PROVIDERS_PATH, directory));
        let skills = server.get::<Vec<SkillInfo>>(SKILLS_PATH, directory);
        (
            models.join().ok().and_then(Result::ok).unwrap_or_default(),
            default.join().ok().and_then(Result::ok).flatten(),
            commands
                .join()
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default(),
            providers
                .join()
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default(),
            skills.unwrap_or_default(),
        )
    });
    let providers = provider_names(providers);
    let windows = models
        .iter()
        .filter(|model| model.limit.context > 0)
        .map(|model| (model.selector(), model.limit.context))
        .collect();
    let offered = commands
        .iter()
        .map(|command| CommandOffer {
            name: command.name.clone(),
            description: command.description.clone().unwrap_or_default(),
        })
        .chain(skills.iter().map(|skill| CommandOffer {
            name: skill.id.clone(),
            description: skill.description.clone().unwrap_or_default(),
        }))
        .collect();
    Listing {
        catalog: Catalog {
            models: offers(&models, default.as_ref(), &providers),
            commands: offered,
            subscription: None,
        },
        windows,
        commands: commands.into_iter().map(|command| command.name).collect(),
        skills: skills.into_iter().map(|skill| skill.id).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::Effort;
    use serde_json::json;

    fn models() -> Vec<ModelInfo> {
        serde_json::from_value(json!([
            {"id": "big-pickle", "providerID": "opencode", "name": "Big Pickle", "variants": [],
             "limit": {"context": 200_000}, "enabled": true, "status": "active"},
            {"id": "deepseek-v4.1-flash-free", "providerID": "opencode", "name": "DeepSeek V4.1 Flash Free",
             "variants": [{"id": "low"}, {"id": "high"}, {"id": "max"}, {"id": "minimal"}],
             "limit": {"context": 200_000}, "enabled": true, "status": "active"},
            {"id": "old", "providerID": "opencode", "status": "deprecated", "enabled": true},
            {"id": "off", "providerID": "opencode", "enabled": false}
        ]))
        .unwrap_or_else(|_| unreachable!("the fixture is valid"))
    }

    #[test]
    fn the_default_model_comes_first_and_old_ones_are_left_out() {
        let models = models();
        let providers = HashMap::new();
        let offered = offers(&models, models.get(1), &providers);
        let values: Vec<&str> = offered.iter().map(|offer| offer.value.as_str()).collect();
        assert_eq!(
            values,
            ["opencode/deepseek-v4.1-flash-free", "opencode/big-pickle"]
        );
        assert_eq!(offered[0].efforts, [Effort::Low, Effort::High, Effort::Max]);
        assert_eq!(offered[1].name, "Big Pickle");
    }

    #[test]
    fn models_are_told_apart_by_plan_and_a_free_model_shows_once() {
        let models: Vec<ModelInfo> = serde_json::from_value(json!([
            {"id": "kimi-k3", "providerID": "opencode-go", "cost": [{"input": 3, "output": 15}]},
            {"id": "claude-opus-5-5", "providerID": "opencode", "cost": [{"input": 4, "output": 20}]},
            {"id": "beam-free", "providerID": "opencode-go", "cost": [{"input": 0, "output": 0}]},
            {"id": "beam-free", "providerID": "opencode", "cost": [{"input": 0, "output": 0}]},
            {"id": "claude-sonnet-5-5", "providerID": "anthropic", "cost": [{"input": 2, "output": 10}]}
        ]))
        .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        let providers = HashMap::from([("anthropic".to_owned(), "Anthropic".to_owned())]);
        let offered: Vec<(String, Option<ModelSource>)> = offers(&models, None, &providers)
            .into_iter()
            .map(|offer| (offer.value, offer.source))
            .collect();
        assert_eq!(
            offered,
            [
                (
                    "opencode-go/kimi-k3".to_owned(),
                    Some(ModelSource::OpencodeGo)
                ),
                (
                    "opencode/claude-opus-5-5".to_owned(),
                    Some(ModelSource::OpencodeZen)
                ),
                ("opencode/beam-free".to_owned(), Some(ModelSource::Free)),
                (
                    "anthropic/claude-sonnet-5-5".to_owned(),
                    Some(ModelSource::Provider("Anthropic".to_owned()))
                ),
            ]
        );
    }
}
