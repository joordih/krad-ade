use gpui_kit::Context;
use krad_domain::{AgentPreset, ModelId, ModelPreferences};

use super::super::EnvironmentShell;
use super::{Choosing, HarnessModels};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    Favorite,
    NewChatDefault,
}

pub(in crate::environment_shell) fn mark(
    group: &mut HarnessModels,
    preferences: &ModelPreferences,
) {
    let default = preferences.default_for(group.harness);
    for choice in &mut group.models {
        let Some(id) = choice.id.as_ref() else {
            continue;
        };
        choice.marks.favorite = preferences.is_favorite(group.harness, id);
        choice.marks.default = default == Some(id);
    }
}

impl Choosing {
    fn marked_model(&self, index: usize) -> Option<(AgentPreset, ModelId)> {
        let (harness, choice) = self.entries().get(index).copied()?;
        Some((harness, choice.id.clone()?))
    }

    fn remark(&mut self, preferences: &ModelPreferences) {
        for group in &mut self.groups {
            mark(group, preferences);
        }
    }
}

impl EnvironmentShell {
    pub(super) fn toggle_model_mark(&mut self, index: usize, which: Mark, cx: &mut Context<Self>) {
        let Some((harness, model)) = self
            .model_picker
            .as_ref()
            .and_then(|picker| picker.choosing.marked_model(index))
        else {
            return;
        };
        let preferences = &mut self.settings.model_preferences;
        match which {
            Mark::Favorite => preferences.toggle_favorite(harness, &model),
            Mark::NewChatDefault => preferences.toggle_default(harness, &model),
        };
        if let Some(picker) = self.model_picker.as_mut() {
            picker.choosing.remark(&self.settings.model_preferences);
        }
        self.save_settings(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{grouped, ModelChoice};
    use super::*;

    fn model(name: &str) -> ModelId {
        ModelId::parse(name).unwrap_or_else(|| unreachable!("{name} is a model id"))
    }

    fn choice(name: &str) -> (AgentPreset, ModelChoice) {
        (
            AgentPreset::ClaudeCode,
            ModelChoice {
                id: Some(model(name)),
                label: name.to_owned(),
                ..ModelChoice::default()
            },
        )
    }

    #[test]
    fn favorites_come_first_and_the_default_is_marked() {
        let mut preferences = ModelPreferences::default();
        preferences.toggle_favorite(AgentPreset::ClaudeCode, &model("haiku"));
        preferences.toggle_default(AgentPreset::ClaudeCode, &model("opus"));
        let mut groups = grouped([choice("opus"), choice("fable"), choice("haiku")]);
        for group in &mut groups {
            mark(group, &preferences);
        }
        let mut picker = Choosing::new(groups).unwrap_or_else(|| unreachable!("there are models"));
        let shown: Vec<(&str, bool, bool)> = picker
            .shown()
            .iter()
            .map(|choice| {
                (
                    choice.label.as_str(),
                    choice.marks.favorite,
                    choice.marks.default,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("haiku", true, false),
                ("opus", false, true),
                ("fable", false, false)
            ]
        );
        preferences.toggle_favorite(AgentPreset::ClaudeCode, &model("haiku"));
        picker.remark(&preferences);
        let labels: Vec<&str> = picker
            .shown()
            .iter()
            .map(|choice| choice.label.as_str())
            .collect();
        assert_eq!(labels, ["opus", "fable", "haiku"]);
        assert_eq!(
            picker.marked_model(0),
            Some((AgentPreset::ClaudeCode, model("opus")))
        );
    }
}
