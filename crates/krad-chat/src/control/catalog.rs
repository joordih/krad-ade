use krad_domain::Effort;
use serde::Deserialize;

use super::each_entry;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModelSource {
    OpencodeGo,
    OpencodeZen,
    Provider(String),
    Free,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "ModelOfferLine")]
pub struct ModelOffer {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
    pub efforts: Vec<Effort>,
    pub thinking_off: bool,
    pub source: Option<ModelSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelOfferLine {
    value: String,
    display_name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    supported_effort_levels: Vec<String>,
    #[serde(default)]
    supports_thinking_off: bool,
}

impl From<ModelOfferLine> for ModelOffer {
    fn from(line: ModelOfferLine) -> Self {
        Self {
            name: line.display_name.unwrap_or_else(|| line.value.clone()),
            value: line.value,
            description: line.description,
            efforts: line
                .supported_effort_levels
                .iter()
                .filter_map(|level| level.parse().ok())
                .collect(),
            thinking_off: line.supports_thinking_off,
            source: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CommandOffer {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "CatalogLine")]
pub struct Catalog {
    pub models: Vec<ModelOffer>,
    pub commands: Vec<CommandOffer>,
    pub subscription: Option<String>,
}

#[derive(Deserialize)]
struct CatalogLine {
    #[serde(default, deserialize_with = "each_entry")]
    models: Vec<ModelOffer>,
    #[serde(default, deserialize_with = "each_entry")]
    commands: Vec<CommandOffer>,
    account: Option<Account>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    subscription_type: Option<String>,
}

impl From<CatalogLine> for Catalog {
    fn from(line: CatalogLine) -> Self {
        Self {
            models: line.models,
            commands: line.commands,
            subscription: line.account.and_then(|account| account.subscription_type),
        }
    }
}
