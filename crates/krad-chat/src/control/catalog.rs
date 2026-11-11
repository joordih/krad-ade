use krad_domain::{
    claude_alias_name, humanize_model_id, split_context, ContextSize, Effort, CLAUDE_BRAND,
};
use serde::Deserialize;

use super::each_entry;

const DEFAULT_MODEL: &str = "default";
const CLAUDE_ID_PREFIX: &str = "claude";
const ANTHROPIC: &str = "Anthropic";
const SUMMARY_SEPARATOR: &str = "\u{b7}";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModelSource {
    Plan { name: String, order: usize },
    Provider(String),
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceRank {
    Plan(usize),
    Provider,
    Free,
}

impl ModelSource {
    #[must_use]
    pub const fn rank(&self) -> SourceRank {
        match self {
            Self::Plan { order, .. } => SourceRank::Plan(*order),
            Self::Provider(_) => SourceRank::Provider,
            Self::Free => SourceRank::Free,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "ModelOfferLine")]
pub struct ModelOffer {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
    pub efforts: Vec<Effort>,
    pub default_effort: Option<Effort>,
    pub thinking_off: bool,
    pub source: Option<ModelSource>,
    pub context: Option<ContextSize>,
    pub recommended: bool,
    pub aliases: Vec<String>,
}

impl ModelOffer {
    #[must_use]
    pub fn answers_to(&self, value: &str) -> bool {
        self.value == value || self.aliases.iter().any(|alias| alias == value)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelOfferLine {
    value: String,
    resolved_model: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    supported_effort_levels: Vec<String>,
    #[serde(default)]
    supports_thinking_off: bool,
}

impl ModelOfferLine {
    fn resolved(&self) -> &str {
        self.resolved_model.as_deref().unwrap_or(&self.value)
    }

    fn summary(&self) -> (Option<&str>, Option<&str>) {
        let Some(description) = self.description.as_deref().map(str::trim) else {
            return (None, None);
        };
        match description.split_once(SUMMARY_SEPARATOR) {
            Some((head, rest)) => (Some(head.trim()), Some(rest.trim())),
            None => (None, Some(description)),
        }
    }

    fn versioned_display_name(&self) -> Option<&str> {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| self.value != DEFAULT_MODEL && name.chars().any(|c| c.is_ascii_digit()))
    }

    fn name(&self) -> String {
        if let Some(name) = self.versioned_display_name() {
            return name.to_owned();
        }
        let resolved = self.resolved();
        if resolved.to_ascii_lowercase().starts_with(CLAUDE_ID_PREFIX) {
            if let Some(name) = humanize_model_id(resolved) {
                return name;
            }
        }
        let alias = claude_alias_name(&self.value);
        let (head, _) = self.summary();
        if let Some(head) = head.filter(|head| head.chars().any(|c| c.is_ascii_digit())) {
            return format!("{CLAUDE_BRAND} {head}");
        }
        alias
            .map(str::to_owned)
            .or_else(|| self.display_name.clone())
            .unwrap_or_else(|| self.value.clone())
    }
}

impl From<ModelOfferLine> for ModelOffer {
    fn from(line: ModelOfferLine) -> Self {
        let (_, context) = split_context(&line.value);
        let (_, summary) = line.summary();
        Self {
            name: line.name(),
            description: summary.map(str::to_owned),
            efforts: Effort::ordered(
                line.supported_effort_levels
                    .iter()
                    .filter_map(|level| level.parse().ok()),
            ),
            thinking_off: line.supports_thinking_off,
            source: Some(ModelSource::Provider(ANTHROPIC.to_owned())),
            context,
            recommended: line.value == DEFAULT_MODEL,
            value: line.value,
            ..Self::default()
        }
    }
}

fn claude_offers(lines: Vec<ModelOfferLine>) -> Vec<ModelOffer> {
    let default = lines
        .iter()
        .find(|line| line.value == DEFAULT_MODEL)
        .map(|line| (line.resolved().to_owned(), split_context(&line.value).1));
    let twin = |line: &ModelOfferLine| {
        default.as_ref().is_some_and(|(resolved, context)| {
            line.value != DEFAULT_MODEL
                && line.resolved() == resolved
                && split_context(&line.value).1 == *context
        })
    };
    let has_twin = lines.iter().any(twin);
    lines
        .into_iter()
        .filter(|line| !(has_twin && line.value == DEFAULT_MODEL))
        .map(|line| {
            let recommended = twin(&line);
            let mut offer = ModelOffer::from(line);
            if recommended {
                offer.recommended = true;
                offer.aliases.push(DEFAULT_MODEL.to_owned());
            }
            offer
        })
        .collect()
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
    models: Vec<ModelOfferLine>,
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
            models: claude_offers(line.models),
            commands: line.commands,
            subscription: line.account.and_then(|account| account.subscription_type),
        }
    }
}

#[cfg(test)]
mod tests;
