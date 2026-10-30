use krad_domain::{Effort, GatewayAccess, ModelId};
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;
use ureq::Agent;

use crate::control::{ModelOffer, ModelSource};

const MODELS_PATH: &str = "/v1/models";
const AUTHORIZATION: &str = "Authorization";
const BEARER: &str = "Bearer";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const UNAUTHORIZED: u16 = 401;
const FORBIDDEN: u16 = 403;

const BASE_URL_VAR: &str = "ANTHROPIC_BASE_URL";
const AUTH_TOKEN_VAR: &str = "ANTHROPIC_AUTH_TOKEN";
const MODEL_VARS: [&str; 6] = [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
];
pub(crate) const CLEARED_VARS: [&str; 2] = ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"];

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("the Gateway rejected the key")]
    Rejected,
    #[error("the Gateway answered with status {0}")]
    Status(u16),
    #[error("the Gateway could not be reached: {0}")]
    Unreachable(String),
    #[error("the Gateway sent a model list that could not be read")]
    Unreadable,
}

impl From<ureq::Error> for GatewayError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::StatusCode(UNAUTHORIZED | FORBIDDEN) => Self::Rejected,
            ureq::Error::StatusCode(status) => Self::Status(status),
            ureq::Error::Json(_) => Self::Unreadable,
            error => Self::Unreachable(error.to_string()),
        }
    }
}

#[derive(Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ListedModel>,
}

#[derive(Deserialize)]
struct ListedModel {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

impl ListedModel {
    fn offer(self) -> Option<ModelOffer> {
        let id = ModelId::parse(&self.id)?;
        Some(ModelOffer {
            value: id.as_str().to_owned(),
            name: id.as_str().to_owned(),
            description: None,
            efforts: Effort::ALL.to_vec(),
            thinking_off: false,
            source: self
                .owned_by
                .filter(|owner| !owner.trim().is_empty())
                .map(ModelSource::Provider),
        })
    }
}

/// # Errors
///
/// Fails when the Gateway cannot be reached, rejects the key or answers
/// with something else than a model list.
pub fn gateway_models(access: &GatewayAccess) -> Result<Vec<ModelOffer>, GatewayError> {
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into();
    let list: ModelList = agent
        .get(access.url.join(MODELS_PATH))
        .header(AUTHORIZATION, format!("{BEARER} {}", access.key.expose()))
        .call()?
        .body_mut()
        .read_json()?;
    Ok(read_models(list))
}

fn read_models(list: ModelList) -> Vec<ModelOffer> {
    let mut offers: Vec<ModelOffer> = Vec::with_capacity(list.data.len());
    for offer in list.data.into_iter().filter_map(ListedModel::offer) {
        if !offers.iter().any(|kept| kept.value == offer.value) {
            offers.push(offer);
        }
    }
    offers
}

#[must_use]
pub fn gateway_env(access: &GatewayAccess, model: Option<&ModelId>) -> Vec<(String, String)> {
    let mut env = vec![
        (BASE_URL_VAR.to_owned(), access.url.as_str().to_owned()),
        (AUTH_TOKEN_VAR.to_owned(), access.key.expose().to_owned()),
    ];
    if let Some(model) = model {
        env.extend(
            MODEL_VARS
                .iter()
                .map(|name| ((*name).to_owned(), model.as_str().to_owned())),
        );
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::GatewayKey;

    fn access() -> GatewayAccess {
        GatewayAccess {
            url: "http://127.0.0.1:8317"
                .parse()
                .unwrap_or_else(|_| unreachable!("a loopback address")),
            key: GatewayKey::parse("sk-local").unwrap_or_else(|| unreachable!("not blank")),
        }
    }

    #[test]
    fn claude_code_reaches_the_gateway_with_every_model_set_to_the_chat_one() {
        let model = ModelId::parse("gemini-3-pro").unwrap_or_else(|| unreachable!("valid"));
        let env = gateway_env(&access(), Some(&model));
        let value = |name: &str| {
            env.iter()
                .find(|(var, _)| var == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value(BASE_URL_VAR), Some("http://127.0.0.1:8317"));
        assert_eq!(value(AUTH_TOKEN_VAR), Some("sk-local"));
        for name in MODEL_VARS {
            assert_eq!(value(name), Some("gemini-3-pro"), "{name}");
        }
        assert_eq!(gateway_env(&access(), None).len(), 2);
    }

    #[test]
    fn the_model_list_keeps_valid_unique_ids_with_their_owner() -> serde_json::Result<()> {
        let list: ModelList = serde_json::from_str(
            r#"{"object":"list","data":[
                {"id":"gpt-5","object":"model","owned_by":"openai"},
                {"id":"gpt-5","object":"model","owned_by":"openai"},
                {"id":"bad id","object":"model"},
                {"id":"gemini-3-pro(high)","object":"model","owned_by":""}
            ]}"#,
        )?;
        let offers = read_models(list);
        let ids: Vec<&str> = offers.iter().map(|offer| offer.value.as_str()).collect();
        assert_eq!(ids, ["gpt-5"]);
        assert_eq!(
            offers[0].source,
            Some(ModelSource::Provider("openai".to_owned()))
        );
        Ok(())
    }
}
