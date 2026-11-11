use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{lenient_model_lists, ModelId};
use crate::agent::AgentPreset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marked {
    On,
    Off,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPreferences {
    #[serde(
        default,
        deserialize_with = "lenient_model_lists",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub favorites: HashMap<AgentPreset, Vec<ModelId>>,
    #[serde(
        default,
        deserialize_with = "lenient_defaults",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub defaults: HashMap<AgentPreset, ModelId>,
}

fn lenient_defaults<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<AgentPreset, ModelId>, D::Error> {
    let names = HashMap::<AgentPreset, String>::deserialize(deserializer)?;
    Ok(names
        .into_iter()
        .filter_map(|(harness, name)| Some((harness, ModelId::parse(&name)?)))
        .collect())
}

impl ModelPreferences {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.favorites.is_empty() && self.defaults.is_empty()
    }

    #[must_use]
    pub fn is_favorite(&self, harness: AgentPreset, model: &ModelId) -> bool {
        self.favorites
            .get(&harness)
            .is_some_and(|favorites| favorites.contains(model))
    }

    pub fn toggle_favorite(&mut self, harness: AgentPreset, model: &ModelId) -> Marked {
        let favorites = self.favorites.entry(harness).or_default();
        let marked = if let Some(at) = favorites.iter().position(|favorite| favorite == model) {
            favorites.remove(at);
            Marked::Off
        } else {
            favorites.push(model.clone());
            Marked::On
        };
        if favorites.is_empty() {
            self.favorites.remove(&harness);
        }
        marked
    }

    #[must_use]
    pub fn default_for(&self, harness: AgentPreset) -> Option<&ModelId> {
        self.defaults.get(&harness)
    }

    pub fn toggle_default(&mut self, harness: AgentPreset, model: &ModelId) -> Marked {
        if self.defaults.get(&harness) == Some(model) {
            self.defaults.remove(&harness);
            return Marked::Off;
        }
        self.defaults.insert(harness, model.clone());
        Marked::On
    }

    #[must_use]
    pub fn new_chat_model(
        &self,
        harness: AgentPreset,
        last_used: Option<&ModelId>,
        offered: Option<&dyn Fn(&ModelId) -> bool>,
    ) -> Option<ModelId> {
        let still_offered = |model: &&ModelId| offered.is_none_or(|offered| offered(model));
        self.default_for(harness)
            .filter(still_offered)
            .or_else(|| last_used.filter(still_offered))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(name: &str) -> ModelId {
        ModelId::parse(name).unwrap_or_else(|| unreachable!("{name} is a model id"))
    }

    #[test]
    fn a_favorite_toggles_on_and_off_and_leaves_no_empty_list() {
        let mut preferences = ModelPreferences::default();
        let fable = model("fable");
        assert_eq!(
            preferences.toggle_favorite(AgentPreset::ClaudeCode, &fable),
            Marked::On
        );
        assert!(preferences.is_favorite(AgentPreset::ClaudeCode, &fable));
        assert!(!preferences.is_favorite(AgentPreset::Codex, &fable));
        assert_eq!(
            preferences.toggle_favorite(AgentPreset::ClaudeCode, &fable),
            Marked::Off
        );
        assert!(preferences.is_empty());
    }

    #[test]
    fn picking_the_default_again_clears_it() {
        let mut preferences = ModelPreferences::default();
        let opus = model("opus");
        assert_eq!(
            preferences.toggle_default(AgentPreset::ClaudeCode, &opus),
            Marked::On
        );
        assert_eq!(
            preferences.toggle_default(AgentPreset::ClaudeCode, &model("haiku")),
            Marked::On
        );
        assert_eq!(
            preferences.default_for(AgentPreset::ClaudeCode),
            Some(&model("haiku"))
        );
        assert_eq!(
            preferences.toggle_default(AgentPreset::ClaudeCode, &model("haiku")),
            Marked::Off
        );
        assert_eq!(preferences.default_for(AgentPreset::ClaudeCode), None);
    }

    #[test]
    fn new_chats_take_the_default_then_the_last_model_while_still_offered() {
        let mut preferences = ModelPreferences::default();
        let stale = model("claude-fable-5-1[1m]");
        let offers = [model("fable"), model("opus")];
        let offered = |candidate: &ModelId| offers.contains(candidate);
        assert_eq!(
            preferences.new_chat_model(AgentPreset::ClaudeCode, Some(&stale), Some(&offered)),
            None,
            "a model the harness stopped offering is not carried into a new chat"
        );
        assert_eq!(
            preferences.new_chat_model(AgentPreset::ClaudeCode, Some(&stale), None),
            Some(stale.clone()),
            "without a listing the last model is kept"
        );
        preferences.toggle_default(AgentPreset::ClaudeCode, &model("opus"));
        assert_eq!(
            preferences.new_chat_model(
                AgentPreset::ClaudeCode,
                Some(&model("fable")),
                Some(&offered)
            ),
            Some(model("opus"))
        );
        preferences.toggle_default(AgentPreset::Codex, &model("gone"));
        assert_eq!(
            preferences.new_chat_model(AgentPreset::Codex, Some(&model("fable")), Some(&offered)),
            Some(model("fable"))
        );
    }

    #[test]
    fn each_acp_agent_keeps_its_own_default_apart_from_the_native_harness() {
        let acp = |id: &str| {
            AgentPreset::Acp(id.parse().unwrap_or_else(|_| unreachable!("{id} is an id")))
        };
        let claude_over_acp = acp("claude-code-acp");
        let mut preferences = ModelPreferences::default();
        preferences.toggle_default(AgentPreset::ClaudeCode, &model("claude-fable-5-2"));
        let agent_offers = [model("default"), model("opus")];
        let offered = |candidate: &ModelId| agent_offers.contains(candidate);
        assert_eq!(
            preferences.new_chat_model(claude_over_acp, None, Some(&offered)),
            None,
            "the native harness default never reaches an ACP agent"
        );
        assert_eq!(
            preferences.new_chat_model(
                claude_over_acp,
                Some(&model("claude-fable-5-2")),
                Some(&offered)
            ),
            None,
            "a last model the agent does not list is left out"
        );
        preferences.toggle_default(claude_over_acp, &model("opus"));
        assert_eq!(
            preferences.new_chat_model(claude_over_acp, None, Some(&offered)),
            Some(model("opus"))
        );
        assert_eq!(
            preferences.new_chat_model(acp("codex-acp"), None, Some(&offered)),
            None
        );
        assert_eq!(
            preferences.default_for(AgentPreset::ClaudeCode),
            Some(&model("claude-fable-5-2"))
        );
    }

    #[test]
    fn preferences_read_back_and_skip_broken_names() -> serde_json::Result<()> {
        let mut preferences = ModelPreferences::default();
        preferences.toggle_favorite(AgentPreset::Codex, &model("gpt-x"));
        preferences.toggle_default(AgentPreset::ClaudeCode, &model("fable"));
        let text = serde_json::to_string(&preferences)?;
        assert_eq!(
            serde_json::from_str::<ModelPreferences>(&text)?,
            preferences
        );
        let broken: ModelPreferences = serde_json::from_str(
            r#"{"favorites":{"Codex":["two words","gpt-x"]},"defaults":{"ClaudeCode":"a b"}}"#,
        )?;
        assert_eq!(
            broken.favorites.get(&AgentPreset::Codex),
            Some(&vec![model("gpt-x")])
        );
        assert!(broken.defaults.is_empty());
        assert_eq!(
            serde_json::from_str::<ModelPreferences>("{}")?,
            ModelPreferences::default()
        );
        Ok(())
    }
}
