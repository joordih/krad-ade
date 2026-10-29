use serde::{Deserialize, Serialize};

const MODEL_PUNCTUATION: &str = "._/:@-";

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(String);

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

    fn is_valid(&self) -> bool {
        Self::parse(&self.0).is_some_and(|parsed| parsed == *self)
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) fn clean_models(models: &[ModelId]) -> Vec<ModelId> {
    let mut clean: Vec<ModelId> = Vec::with_capacity(models.len());
    for model in models.iter().filter(|model| model.is_valid()) {
        if !clean.contains(model) {
            clean.push(model.clone());
        }
    }
    clean
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Effort {
    Low,
    Medium,
    High,
    ExtraHigh,
    Max,
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
    pub const ALL: [Self; 5] = [
        Self::Low,
        Self::Medium,
        Self::High,
        Self::ExtraHigh,
        Self::Max,
    ];

    const fn level(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::ExtraHigh => "xhigh",
            Self::Max => "max",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::ExtraHigh => "Extra high",
            Self::Max => "Max",
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
        Self::ALL
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
    }

    #[test]
    fn stored_lists_drop_invalid_and_repeated_names() {
        let stored = vec![
            ModelId("opus".to_owned()),
            ModelId(" ".to_owned()),
            ModelId("opus".to_owned()),
            ModelId("two words".to_owned()),
            ModelId("haiku".to_owned()),
        ];
        let clean: Vec<String> = clean_models(&stored)
            .into_iter()
            .map(|model| model.0)
            .collect();
        assert_eq!(clean, ["opus", "haiku"]);
    }

    #[test]
    fn effort_levels_read_and_write_as_claude_code_has_them() {
        assert_eq!("xhigh".parse(), Ok(Effort::ExtraHigh));
        assert_eq!("medium".parse(), Ok(Effort::Medium));
        assert_eq!("turbo".parse::<Effort>(), Err(UnknownEffort));
        for effort in Effort::ALL {
            assert_eq!(effort.to_string().parse(), Ok(effort));
        }
    }
}
