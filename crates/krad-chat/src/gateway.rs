use krad_domain::{humanize_model_id, split_context, GatewayAccess, ModelId};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use ureq::Agent;

use crate::control::{ModelOffer, ModelSource};
use crate::engine::Protocol;

const MODELS_PATH: &str = "/v1/models";
pub(crate) const AUTHORIZATION: &str = "Authorization";
pub(crate) const BEARER: &str = "Bearer";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const UNAUTHORIZED: u16 = 401;
const FORBIDDEN: u16 = 403;
const DETAIL_SEPARATOR: &str = ": ";
const ANTHROPIC_OWNERS: [&str; 2] = ["anthropic", "claude"];
const OPENAI_OWNERS: [&str; 3] = ["openai", "codex", "azure-openai"];
const GOOGLE_OWNERS: [&str; 2] = ["google", "gemini"];
const ANTHROPIC_LABEL: &str = "Anthropic";
const OPENAI_LABEL: &str = "OpenAI";
const GOOGLE_LABEL: &str = "Google";

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("the Gateway rejected the key")]
    Rejected,
    #[error("the Gateway answered with status {code}{}", detail_suffix(.detail))]
    Status { code: u16, detail: String },
    #[error("the Gateway could not be reached: {0}")]
    Unreachable(String),
    #[error("the Gateway sent an answer that could not be read: {0}")]
    Unreadable(String),
    #[error("the model stopped with an error: {0}")]
    Failed(String),
    #[error("the answer was stopped")]
    Cancelled,
}

fn detail_suffix(detail: &str) -> String {
    if detail.is_empty() {
        return String::new();
    }
    format!("{DETAIL_SEPARATOR}{detail}")
}

impl GatewayError {
    pub(crate) const fn rejects(code: u16) -> bool {
        matches!(code, UNAUTHORIZED | FORBIDDEN)
    }
}

impl From<ureq::Error> for GatewayError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::StatusCode(code) if Self::rejects(code) => Self::Rejected,
            ureq::Error::StatusCode(code) => Self::Status {
                code,
                detail: String::new(),
            },
            ureq::Error::Json(error) => Self::Unreadable(error.to_string()),
            error => Self::Unreachable(error.to_string()),
        }
    }
}

impl From<serde_json::Error> for GatewayError {
    fn from(error: serde_json::Error) -> Self {
        Self::Unreadable(error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vendor {
    Anthropic,
    OpenAi,
    Google,
    #[default]
    Other,
}

impl Vendor {
    #[must_use]
    pub const fn label(self) -> Option<&'static str> {
        match self {
            Self::Anthropic => Some(ANTHROPIC_LABEL),
            Self::OpenAi => Some(OPENAI_LABEL),
            Self::Google => Some(GOOGLE_LABEL),
            Self::Other => None,
        }
    }
}

fn owned_by(owner: &str, known: &[&str]) -> bool {
    known.iter().any(|name| owner.eq_ignore_ascii_case(name))
}

impl From<&str> for Vendor {
    fn from(owner: &str) -> Self {
        let owner = owner.trim();
        if owned_by(owner, &ANTHROPIC_OWNERS) {
            Self::Anthropic
        } else if owned_by(owner, &OPENAI_OWNERS) {
            Self::OpenAi
        } else if owned_by(owner, &GOOGLE_OWNERS) {
            Self::Google
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayModel {
    pub offer: ModelOffer,
    pub vendor: Vendor,
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
    #[serde(default)]
    display_name: Option<String>,
}

impl ListedModel {
    fn model(self) -> Option<GatewayModel> {
        let id = ModelId::parse(&self.id)?;
        let owner = self.owned_by.filter(|owner| !owner.trim().is_empty());
        let vendor = owner.as_deref().map_or_default(Vendor::from);
        let name = self
            .display_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .or_else(|| humanize_model_id(id.as_str()))
            .unwrap_or_else(|| id.as_str().to_owned());
        Some(GatewayModel {
            offer: ModelOffer {
                value: id.as_str().to_owned(),
                name,
                efforts: Protocol::from(vendor).efforts().to_vec(),
                source: vendor
                    .label()
                    .map(str::to_owned)
                    .or(owner)
                    .map(ModelSource::Provider),
                context: split_context(id.as_str()).1,
                ..ModelOffer::default()
            },
            vendor,
        })
    }
}

/// # Errors
///
/// Fails when the Gateway cannot be reached, rejects the key or answers
/// with something else than a model list.
pub fn gateway_models(access: &GatewayAccess) -> Result<Vec<GatewayModel>, GatewayError> {
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

fn read_models(list: ModelList) -> Vec<GatewayModel> {
    let mut models: Vec<GatewayModel> = Vec::with_capacity(list.data.len());
    for model in list.data.into_iter().filter_map(ListedModel::model) {
        if !models
            .iter()
            .any(|kept| kept.offer.value == model.offer.value)
        {
            models.push(model);
        }
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::Effort;

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
        let models = read_models(list);
        let ids: Vec<&str> = models
            .iter()
            .map(|model| model.offer.value.as_str())
            .collect();
        assert_eq!(ids, ["gpt-5"]);
        assert_eq!(
            models[0].offer.source,
            Some(ModelSource::Provider(OPENAI_LABEL.to_owned()))
        );
        assert_eq!(models[0].offer.name, "GPT-5");
        Ok(())
    }

    #[test]
    fn a_display_name_from_the_gateway_wins_over_the_humanized_id() -> serde_json::Result<()> {
        let list: ModelList = serde_json::from_str(
            r#"{"data":[
                {"id":"claude-fable-5-2","owned_by":"anthropic","display_name":"Claude Fable 5.2"},
                {"id":"gpt-7-nova","owned_by":"openai","display_name":"  "}
            ]}"#,
        )?;
        let names: Vec<String> = read_models(list)
            .into_iter()
            .map(|model| model.offer.name)
            .collect();
        assert_eq!(names, ["Claude Fable 5.2", "GPT-7 Nova"]);
        Ok(())
    }

    #[test]
    fn the_owner_alone_picks_the_vendor_and_the_efforts_it_offers() -> serde_json::Result<()> {
        let list: ModelList = serde_json::from_str(
            r#"{"object":"list","data":[
                {"id":"claude-sonnet-5","owned_by":"Anthropic"},
                {"id":"gpt-oss-120b","owned_by":"meta"},
                {"id":"openai/gpt-5","owned_by":"openrouter"},
                {"id":"o3","owned_by":"codex"},
                {"id":"gemini-3-pro","owned_by":"google"},
                {"id":"claude-via-nobody"}
            ]}"#,
        )?;
        let read: Vec<(String, Vendor, Vec<Effort>)> = read_models(list)
            .into_iter()
            .map(|model| (model.offer.value, model.vendor, model.offer.efforts))
            .collect();
        let few = vec![Effort::Low, Effort::Medium, Effort::High];
        let expected = [
            ("claude-sonnet-5", Vendor::Anthropic, Effort::ALL.to_vec()),
            ("gpt-oss-120b", Vendor::Other, few.clone()),
            ("openai/gpt-5", Vendor::Other, few.clone()),
            ("o3", Vendor::OpenAi, few.clone()),
            ("gemini-3-pro", Vendor::Google, few.clone()),
            ("claude-via-nobody", Vendor::Other, few),
        ]
        .map(|(id, vendor, efforts)| (id.to_owned(), vendor, efforts));
        assert_eq!(read, expected);
        Ok(())
    }
}
