use krad_domain::{
    AcpConfigCategory, AcpConfigChoice, AcpConfigOption, AcpConfigValue, AcpMode, AcpModeId,
    AcpModel, AcpModelId, AcpModels, AcpModes,
};

use crate::acp::config::{ConfigKind, SessionConfigOption};
use crate::acp::ids::ConfigOptionId;
use crate::acp::setup::{SessionModeState, SessionModelState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Setter {
    Method,
    Config(ConfigOptionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Offered<T> {
    pub(super) offer: T,
    pub(super) setter: Setter,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Choices {
    pub(super) modes: Option<Offered<AcpModes>>,
    pub(super) models: Option<Offered<AcpModels>>,
    pub(super) others: Vec<AcpConfigOption>,
}

impl From<SessionModeState> for AcpModes {
    fn from(state: SessionModeState) -> Self {
        Self {
            current: state.current_mode_id,
            available: state
                .available_modes
                .into_iter()
                .map(|mode| AcpMode {
                    id: mode.id,
                    name: mode.name,
                    description: mode.description,
                })
                .collect(),
        }
    }
}

impl From<SessionModelState> for AcpModels {
    fn from(state: SessionModelState) -> Self {
        Self {
            current: state.current_model_id,
            available: state
                .available_models
                .into_iter()
                .map(|model| AcpModel {
                    id: model.model_id,
                    name: model.name,
                    description: model.description,
                })
                .collect(),
        }
    }
}

fn configured_modes(option: &SessionConfigOption) -> AcpModes {
    AcpModes {
        current: AcpModeId(option.current_value.clone()),
        available: option
            .choices()
            .map(|choice| AcpMode {
                id: AcpModeId(choice.value.clone()),
                name: choice.name.clone(),
                description: choice.description.clone(),
            })
            .collect(),
    }
}

fn configured_models(option: &SessionConfigOption) -> AcpModels {
    AcpModels {
        current: AcpModelId(option.current_value.clone()),
        available: option
            .choices()
            .map(|choice| AcpModel {
                id: AcpModelId(choice.value.clone()),
                name: choice.name.clone(),
                description: choice.description.clone(),
            })
            .collect(),
    }
}

fn other_option(option: &SessionConfigOption) -> AcpConfigOption {
    AcpConfigOption {
        id: option.id.clone(),
        name: option.name.clone(),
        description: option.description.clone(),
        category: option.category,
        current: AcpConfigValue(option.current_value.clone()),
        choices: option
            .choices()
            .map(|choice| AcpConfigChoice {
                value: AcpConfigValue(choice.value.clone()),
                name: choice.name.clone(),
                description: choice.description.clone(),
            })
            .collect(),
    }
}

fn selected(
    options: &[SessionConfigOption],
    category: AcpConfigCategory,
) -> Option<SessionConfigOption> {
    options
        .iter()
        .find(|option| option.kind == ConfigKind::Select && option.category == Some(category))
        .cloned()
}

impl Choices {
    pub(super) fn reported(
        reported_modes: Option<SessionModeState>,
        offered_models: Option<SessionModelState>,
        config: &[SessionConfigOption],
    ) -> Self {
        let mut choices = Self {
            modes: reported_modes.map(|state| Offered {
                offer: state.into(),
                setter: Setter::Method,
            }),
            models: offered_models.map(|state| Offered {
                offer: state.into(),
                setter: Setter::Method,
            }),
            others: Vec::new(),
        };
        choices.configure(config);
        choices
    }

    pub(super) fn configure(&mut self, config: &[SessionConfigOption]) {
        let mode = selected(config, AcpConfigCategory::Mode);
        let model = selected(config, AcpConfigCategory::Model);
        let taken = |option: &SessionConfigOption| {
            [&mode, &model]
                .into_iter()
                .flatten()
                .any(|taken| taken.id == option.id)
        };
        self.others = config
            .iter()
            .filter(|option| option.kind == ConfigKind::Select && !taken(option))
            .map(other_option)
            .collect();
        if let Some(option) = mode {
            self.modes = Some(Offered {
                offer: configured_modes(&option),
                setter: Setter::Config(option.id),
            });
        }
        if let Some(option) = model {
            self.models = Some(Offered {
                offer: configured_models(&option),
                setter: Setter::Config(option.id),
            });
        }
    }

    pub(super) fn mode_is(&mut self, current: AcpModeId) -> Option<&AcpModes> {
        let modes = &mut self.modes.as_mut()?.offer;
        modes.current = current;
        Some(modes)
    }

    pub(super) fn offers_other(&self, id: &ConfigOptionId, value: &AcpConfigValue) -> bool {
        self.others
            .iter()
            .any(|option| &option.id == id && option.offers(value))
    }

    pub(super) fn other_is_already(&self, id: &ConfigOptionId, value: &AcpConfigValue) -> bool {
        self.others
            .iter()
            .any(|option| &option.id == id && &option.current == value)
    }

    pub(super) fn option_is(&mut self, id: &ConfigOptionId, current: AcpConfigValue) {
        if let Some(option) = self.others.iter_mut().find(|option| &option.id == id) {
            option.current = current;
        }
    }

    pub(super) fn model_is(&mut self, current: AcpModelId) -> Option<&AcpModels> {
        let models = &mut self.models.as_mut()?.offer;
        models.current = current;
        Some(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::AcpOptionRole;
    use serde_json::json;

    fn config(value: serde_json::Value) -> Vec<SessionConfigOption> {
        serde_json::from_value(value).unwrap_or_default()
    }

    #[test]
    fn config_options_win_over_the_mode_and_model_lists() {
        let modes: Option<SessionModeState> = serde_json::from_value(json!({
            "currentModeId": "ask",
            "availableModes": [{"id": "ask", "name": "Ask"}]
        }))
        .ok();
        let options = config(json!([
            {"id": "mode", "name": "Session Mode", "category": "mode", "type": "select",
             "currentValue": "build",
             "options": [{"value": "build", "name": "Build"}, {"value": "plan", "name": "Plan"}]},
            {"id": "model", "name": "Model", "category": "model", "type": "select",
             "currentValue": "big-pickle", "options": [{"value": "big-pickle", "name": "Big Pickle"}]},
            {"id": "effort", "name": "Thought level", "category": "thought_level", "type": "select",
             "currentValue": "high", "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}
        ]));
        let choices = Choices::reported(modes, None, &options);
        let others: Vec<(&str, &str)> = choices
            .others
            .iter()
            .map(|option| (option.id.0.as_str(), option.current.0.as_str()))
            .collect();
        assert_eq!(others, [("effort", "high")]);
        assert!(choices.offers_other(
            &ConfigOptionId("effort".to_owned()),
            &AcpConfigValue("low".to_owned())
        ));
        assert!(!choices.offers_other(
            &ConfigOptionId("mode".to_owned()),
            &AcpConfigValue("build".to_owned())
        ));
        let modes = choices
            .modes
            .as_ref()
            .map(|modes| (&modes.offer.current, &modes.setter));
        assert_eq!(
            modes,
            Some((
                &AcpModeId("build".to_owned()),
                &Setter::Config(ConfigOptionId("mode".to_owned()))
            ))
        );
        assert_eq!(
            choices
                .models
                .and_then(|models| models.offer.current_model().map(|model| model.name.clone())),
            Some("Big Pickle".to_owned())
        );
    }

    #[test]
    fn claude_agent_acp_s_options_keep_their_category_for_the_composer() {
        let options = config(json!([
            {"id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": "auto",
             "options": [{"value": "default", "name": "Manual"}, {"value": "auto", "name": "Auto"}]},
            {"id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": "opus",
             "options": [{"value": "default", "name": "Default (recommended)"}, {"value": "opus", "name": "Opus 5.5"}]},
            {"id": "effort", "name": "Effort", "category": "thought_level", "type": "select",
             "currentValue": "medium",
             "options": [{"value": "default", "name": "Default"}, {"value": "low", "name": "Low"},
                         {"value": "medium", "name": "Medium"}, {"value": "high", "name": "High"},
                         {"value": "xhigh", "name": "Xhigh"}, {"value": "max", "name": "Max"}]},
            {"id": "fast", "name": "Fast mode", "category": "model_config", "type": "select",
             "currentValue": "off", "options": [{"value": "on", "name": "On"}, {"value": "off", "name": "Off"}]},
            {"id": "ultracode", "name": "Ultracode", "category": "model_config", "type": "select",
             "currentValue": "off", "options": [{"value": "on", "name": "On"}, {"value": "off", "name": "Off"}]},
            {"id": "agent", "name": "Agent", "type": "select", "currentValue": "default",
             "options": [{"value": "default", "name": "Default"}]}
        ]));
        let choices = Choices::reported(None, None, &options);
        let roles: Vec<(&str, AcpOptionRole)> = choices
            .others
            .iter()
            .map(|option| (option.id.0.as_str(), option.role()))
            .collect();
        assert_eq!(
            roles,
            [
                ("effort", AcpOptionRole::Effort),
                ("fast", AcpOptionRole::Fast),
                ("ultracode", AcpOptionRole::Ultracode),
                ("agent", AcpOptionRole::Persona),
            ]
        );
        assert!(choices.modes.is_some() && choices.models.is_some());
    }

    #[test]
    fn the_stable_lists_are_set_through_their_own_methods() {
        let models: Option<SessionModelState> = serde_json::from_value(json!({
            "currentModelId": "sonnet",
            "availableModels": [{"modelId": "sonnet", "name": "Sonnet"}, {"modelId": "opus", "name": "Opus"}]
        }))
        .ok();
        let mut choices = Choices::reported(None, models, &[]);
        assert_eq!(
            choices.models.as_ref().map(|models| &models.setter),
            Some(&Setter::Method)
        );
        let current = choices
            .model_is(AcpModelId("opus".to_owned()))
            .map(|models| models.current.clone());
        assert_eq!(current, Some(AcpModelId("opus".to_owned())));
        assert!(choices.mode_is(AcpModeId("x".to_owned())).is_none());
    }
}
