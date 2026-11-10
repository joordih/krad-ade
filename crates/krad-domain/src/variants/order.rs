use crate::agent::AgentPreset;
use crate::harness::WorkerSurface;
use crate::model::ModelId;
use crate::project_task::ProjectTask;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_VARIANTS: usize = 4;
pub const MIN_VARIANTS: usize = 2;
const INSTRUCTION_BREAK: &str = "\n\n";
const LABEL_SEPARATOR: &str = " · ";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantSpec {
    pub harness: AgentPreset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
}

impl VariantSpec {
    #[must_use]
    pub fn new(harness: AgentPreset, model: Option<ModelId>, instruction: Option<&str>) -> Self {
        let instruction = instruction
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
        Self {
            harness,
            model,
            instruction,
        }
    }

    #[must_use]
    pub fn label(&self, harness_name: &str) -> String {
        match &self.model {
            Some(model) => format!("{harness_name}{LABEL_SEPARATOR}{model}"),
            None => harness_name.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantsOrderError {
    EmptyPrompt,
    TooFew,
    TooMany,
    NotAWorker,
}

impl fmt::Display for VariantsOrderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPrompt => formatter.write_str("Write the task every variant works on"),
            Self::TooFew => write!(formatter, "Compare at least {MIN_VARIANTS} variants"),
            Self::TooMany => write!(formatter, "Compare at most {MAX_VARIANTS} variants"),
            Self::NotAWorker => {
                formatter.write_str("Every variant needs an agent that works in a chat")
            }
        }
    }
}

impl std::error::Error for VariantsOrderError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantsOrder {
    pub prompt: String,
    pub variants: Vec<VariantSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<ProjectTask>,
}

impl VariantsOrder {
    /// # Errors
    ///
    /// Returns an error when the prompt is blank or the number of variants is
    /// outside [`MIN_VARIANTS`]..=[`MAX_VARIANTS`].
    pub fn new(
        prompt: &str,
        variants: Vec<VariantSpec>,
        test: Option<ProjectTask>,
    ) -> Result<Self, VariantsOrderError> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Err(VariantsOrderError::EmptyPrompt);
        }
        if variants.len() < MIN_VARIANTS {
            return Err(VariantsOrderError::TooFew);
        }
        if variants.len() > MAX_VARIANTS {
            return Err(VariantsOrderError::TooMany);
        }
        if variants
            .iter()
            .any(|spec| spec.harness.worker_surface() != WorkerSurface::Chat)
        {
            return Err(VariantsOrderError::NotAWorker);
        }
        Ok(Self {
            prompt: prompt.to_owned(),
            variants,
            test,
        })
    }

    #[must_use]
    pub fn brief(prompt: &str, spec: &VariantSpec) -> String {
        match &spec.instruction {
            Some(instruction) => format!("{prompt}{INSTRUCTION_BREAK}{instruction}"),
            None => prompt.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude() -> VariantSpec {
        VariantSpec::new(AgentPreset::ClaudeCode, None, None)
    }

    #[test]
    fn an_order_needs_a_prompt_and_two_to_four_variants() {
        assert_eq!(
            VariantsOrder::new("  ", vec![claude(), claude()], None),
            Err(VariantsOrderError::EmptyPrompt)
        );
        assert_eq!(
            VariantsOrder::new("Fix it", vec![claude()], None),
            Err(VariantsOrderError::TooFew)
        );
        assert_eq!(
            VariantsOrder::new("Fix it", vec![claude(); MAX_VARIANTS + 1], None),
            Err(VariantsOrderError::TooMany)
        );
        let shell = VariantSpec::new(AgentPreset::Shell, None, None);
        assert_eq!(
            VariantsOrder::new("Fix it", vec![claude(), shell], None),
            Err(VariantsOrderError::NotAWorker)
        );
        let order = VariantsOrder::new(" Fix it ", vec![claude(); MAX_VARIANTS], None);
        assert_eq!(order.map(|order| order.prompt), Ok("Fix it".to_owned()));
    }

    #[test]
    fn a_variant_brief_adds_its_own_instruction_after_the_shared_prompt() {
        let terse = VariantSpec::new(AgentPreset::Codex, None, Some("  Keep the diff small "));
        assert_eq!(
            VariantsOrder::brief("Fix the login", &terse),
            "Fix the login\n\nKeep the diff small"
        );
        let blank = VariantSpec::new(AgentPreset::Codex, None, Some("   "));
        assert_eq!(blank.instruction, None);
        assert_eq!(
            VariantsOrder::brief("Fix the login", &blank),
            "Fix the login"
        );
    }

    #[test]
    fn a_variant_is_labelled_by_its_harness_and_model() {
        let model = ModelId::parse("gpt-5.5");
        let spec = VariantSpec::new(AgentPreset::Codex, model, None);
        assert_eq!(spec.label("Codex"), "Codex · gpt-5.5");
        assert_eq!(claude().label("Claude Code"), "Claude Code");
    }
}
