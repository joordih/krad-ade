use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::hash::Hash;

mod choice;
mod effort_scale;
mod name;

pub use choice::ChatChoice;
pub use effort_scale::{snap_effort, EffortSnap};
pub use name::{claude_alias_name, humanize_model_id, split_context, ContextSize, CLAUDE_BRAND};

const MODEL_PUNCTUATION: &str = "._/:@-[]";

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ModelId(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidModelId;

impl std::fmt::Display for InvalidModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a model name is one word of letters, digits and ._/:@-[]")
    }
}

impl std::error::Error for InvalidModelId {}

impl ModelId {
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        let allowed = |c: char| c.is_ascii_alphanumeric() || MODEL_PUNCTUATION.contains(c);
        (!name.is_empty() && name.chars().all(allowed)).then(|| Self(name.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ModelId {
    type Error = InvalidModelId;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::parse(&name).ok_or(InvalidModelId)
    }
}

impl From<ModelId> for String {
    fn from(model: ModelId) -> Self {
        model.0
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn valid_models(names: &[String]) -> Vec<ModelId> {
    let mut valid: Vec<ModelId> = Vec::with_capacity(names.len());
    for model in names.iter().filter_map(|name| ModelId::parse(name)) {
        if !valid.contains(&model) {
            valid.push(model);
        }
    }
    valid
}

pub(crate) fn lenient_models<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ModelId>, D::Error> {
    Vec::<String>::deserialize(deserializer).map(|names| valid_models(&names))
}

pub(crate) fn lenient_model_lists<'de, D, K>(
    deserializer: D,
) -> Result<HashMap<K, Vec<ModelId>>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Eq + Hash,
{
    let lists = HashMap::<K, Vec<String>>::deserialize(deserializer)?;
    Ok(lists
        .into_iter()
        .map(|(key, names)| (key, valid_models(&names)))
        .collect())
}

pub(crate) fn lenient_model<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ModelId>, D::Error> {
    let name = Option::<String>::deserialize(deserializer)?;
    Ok(name.and_then(|name| ModelId::parse(&name)))
}

pub(crate) fn lenient_effort<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Effort>, D::Error> {
    let level = Option::<String>::deserialize(deserializer)?;
    Ok(level.and_then(|level| level.parse().ok()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Minimal,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    ExtraHigh,
    Max,
    Ultra,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownEffort;

impl std::fmt::Display for UnknownEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown effort level")
    }
}

impl std::error::Error for UnknownEffort {}

impl Effort {
    pub const LEVELS: [Self; 7] = [
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::ExtraHigh,
        Self::Max,
        Self::Ultra,
    ];

    pub const ALL: [Self; 5] = [
        Self::Low,
        Self::Medium,
        Self::High,
        Self::ExtraHigh,
        Self::Max,
    ];

    const fn level(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::ExtraHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::ExtraHigh => "Extra high",
            Self::Max => "Max",
            Self::Ultra => "Ultra",
        }
    }

    #[must_use]
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Minimal => "Min",
            Self::Low => "Low",
            Self::Medium => "Med",
            Self::High => "High",
            Self::ExtraHigh => "XHigh",
            Self::Max => "Max",
            Self::Ultra => "Ultra",
        }
    }

    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal: answers almost at once, with barely any reasoning",
            Self::Low => "Low: quick answers with light reasoning",
            Self::Medium => "Medium: balances speed and depth",
            Self::High => "High: reasons longer before answering",
            Self::ExtraHigh => "Extra high: deep reasoning for hard problems",
            Self::Max => "Max: the most reasoning the model allows per turn",
            Self::Ultra => "Ultra: the longest reasoning, for the hardest tasks",
        }
    }
}

impl std::fmt::Display for Effort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.level())
    }
}

impl std::str::FromStr for Effort {
    type Err = UnknownEffort;

    fn from_str(level: &str) -> Result<Self, Self::Err> {
        Self::LEVELS
            .into_iter()
            .find(|effort| effort.level() == level)
            .ok_or(UnknownEffort)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names_are_one_trimmed_word() {
        assert_eq!(
            ModelId::parse("  opus ").map(|m| m.0),
            Some("opus".to_owned())
        );
        assert_eq!(
            ModelId::parse("openai/gpt-5").map(|m| m.to_string()),
            Some("openai/gpt-5".to_owned())
        );
        assert_eq!(ModelId::parse(""), None);
        assert_eq!(ModelId::parse("   "), None);
        assert_eq!(ModelId::parse("claude opus"), None);
        assert_eq!(ModelId::parse("opus&calc"), None);
        assert_eq!(ModelId::parse("\"opus\""), None);
        assert_eq!(
            ModelId::parse("claude-fable-5-1[1m]").map(|m| m.to_string()),
            Some("claude-fable-5-1[1m]".to_owned())
        );
    }

    #[test]
    fn a_model_is_checked_when_read() -> serde_json::Result<()> {
        assert_eq!(
            serde_json::from_str::<ModelId>(r#""openai/gpt-5""#)?,
            ModelId("openai/gpt-5".to_owned())
        );
        assert!(serde_json::from_str::<ModelId>(r#""two words""#).is_err());
        assert_eq!(
            serde_json::to_string(&ModelId("opus".to_owned()))?,
            r#""opus""#
        );
        Ok(())
    }

    #[test]
    fn effort_levels_read_and_write_as_claude_code_has_them() {
        assert_eq!("xhigh".parse(), Ok(Effort::ExtraHigh));
        assert_eq!("medium".parse(), Ok(Effort::Medium));
        assert_eq!("ultra".parse(), Ok(Effort::Ultra));
        assert_eq!("minimal".parse(), Ok(Effort::Minimal));
        assert_eq!("turbo".parse::<Effort>(), Err(UnknownEffort));
        for effort in Effort::LEVELS {
            assert_eq!(effort.to_string().parse(), Ok(effort));
        }
    }
}
