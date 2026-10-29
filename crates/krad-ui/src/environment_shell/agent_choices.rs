use krad_domain::{AgentPreset, ModelId};

use super::EnvironmentShell;
use crate::components::ModelChoice;

impl EnvironmentShell {
    pub(super) fn agent_choices(&self) -> Vec<ModelChoice> {
        AgentPreset::all()
            .into_iter()
            .filter(|preset| preset.is_agent() && preset.model_flag().is_some())
            .map(|preset| ModelChoice {
                preset,
                models: self.settings.models(preset),
                available: self.installed_agents().contains(&preset),
            })
            .collect()
    }

    pub(super) fn default_agent_choice(&self) -> (Option<AgentPreset>, Option<ModelId>) {
        let choices = self.agent_choices();
        let preset = choices
            .iter()
            .find(|choice| choice.available && choice.preset == self.settings.default_harness)
            .or_else(|| choices.iter().find(|choice| choice.available))
            .map(|choice| choice.preset);
        let model = preset.and_then(|preset| self.settings.models(preset).into_iter().next());
        (preset, model)
    }
}
