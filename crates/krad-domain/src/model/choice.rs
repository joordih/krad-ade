use serde::{Deserialize, Serialize};

use super::{lenient_effort, lenient_model, Effort, ModelId};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatChoice {
    #[serde(
        default,
        deserialize_with = "lenient_model",
        skip_serializing_if = "Option::is_none"
    )]
    pub model: Option<ModelId>,
    #[serde(
        default,
        deserialize_with = "lenient_effort",
        skip_serializing_if = "Option::is_none"
    )]
    pub effort: Option<Effort>,
}

impl ChatChoice {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.model.is_none() && self.effort.is_none()
    }

    #[must_use]
    pub fn or(self, fallback: &Self) -> Self {
        Self {
            model: self.model.or_else(|| fallback.model.clone()),
            effort: self.effort.or(fallback.effort),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(name: &str) -> Option<ModelId> {
        ModelId::parse(name)
    }

    #[test]
    fn a_choice_writes_only_what_was_chosen_and_reads_back() -> serde_json::Result<()> {
        let choice = ChatChoice {
            model: model("claude-fable-5-1[1m]"),
            effort: Some(Effort::ExtraHigh),
        };
        let text = serde_json::to_string(&choice)?;
        assert_eq!(text, r#"{"model":"claude-fable-5-1[1m]","effort":"xhigh"}"#);
        assert_eq!(serde_json::from_str::<ChatChoice>(&text)?, choice);
        assert_eq!(serde_json::to_string(&ChatChoice::default())?, "{}");
        Ok(())
    }

    #[test]
    fn unknown_levels_and_broken_models_read_as_unset() -> serde_json::Result<()> {
        let read: ChatChoice = serde_json::from_str(r#"{"model":"two words","effort":"turbo"}"#)?;
        assert!(read.is_empty());
        assert!(serde_json::from_str::<ChatChoice>("{}")?.is_empty());
        Ok(())
    }

    #[test]
    fn what_the_chat_chose_wins_over_the_remembered_default() {
        let remembered = ChatChoice {
            model: model("opus"),
            effort: Some(Effort::High),
        };
        let chosen = ChatChoice {
            model: model("haiku"),
            effort: None,
        };
        assert_eq!(
            chosen.or(&remembered),
            ChatChoice {
                model: model("haiku"),
                effort: Some(Effort::High),
            }
        );
    }
}
