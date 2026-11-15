use serde::{Deserialize, Serialize};

use super::{humanize_model_id, ContextSize, Effort};
use crate::AcpModel;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ModelOffer {
    pub value: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub efforts: Vec<Effort>,
    #[serde(default)]
    pub default_effort: Option<Effort>,
    #[serde(default)]
    pub thinking_off: bool,
    #[serde(default)]
    pub source: Option<ModelSource>,
    #[serde(default)]
    pub context: Option<ContextSize>,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl ModelOffer {
    #[must_use]
    pub fn answers_to(&self, value: &str) -> bool {
        self.value == value || self.aliases.iter().any(|alias| alias == value)
    }
}

impl From<&AcpModel> for ModelOffer {
    fn from(model: &AcpModel) -> Self {
        let name = Some(model.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| humanize_model_id(&model.id.0))
            .unwrap_or_else(|| model.id.0.clone());
        Self {
            value: model.id.0.clone(),
            name,
            description: model.description.clone(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AcpModelId;

    #[test]
    fn an_unnamed_acp_model_shows_its_id() {
        let model = AcpModel {
            id: AcpModelId("x-1".to_owned()),
            name: " ".to_owned(),
            description: None,
        };
        let shown = humanize_model_id("x-1").unwrap_or_else(|| "x-1".to_owned());
        assert_eq!(ModelOffer::from(&model).name, shown);
    }
}
