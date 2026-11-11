mod form;

use gpui_kit::component::input::InputState;
use gpui_kit::{AppContext as _, Context, Entity, Window};
use krad_domain::{
    name_ideas, AgentPreset, Kraddie, KraddieId, KraddieName, KraddieWorkspace, ModelId,
    WorkerSurface, NAME_IDEAS_SHOWN,
};
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

use super::super::EnvironmentShell;

const NAME_PLACEHOLDER: &str = "What should we call it?";
const MODEL_PLACEHOLDER: &str = "Model id, or leave empty for the agent's own pick";
const BAD_MODEL: &str = "That model id is not one the agent knows how to read";
const NO_AGENT: &str = "Install an agent that can run as a worker chat first";

pub(in crate::environment_shell) struct KraddieEditor {
    replacing: Option<KraddieId>,
    base: Option<Kraddie>,
    harness: AgentPreset,
    workspace: Option<KraddieWorkspace>,
    name: Entity<InputState>,
    model: Entity<InputState>,
    ideas: Vec<&'static str>,
    shuffles: u64,
    refusal: Option<String>,
    refusals: usize,
}

fn fresh_seed() -> u64 {
    RandomState::new().hash_one(std::time::SystemTime::now())
}

fn read_model(text: &str) -> Result<Option<ModelId>, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    ModelId::try_from(text.to_owned())
        .map(Some)
        .map_err(|_| BAD_MODEL)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn kraddie_agents(&self) -> Vec<AgentPreset> {
        let mut agents: Vec<AgentPreset> = self
            .offered_agents()
            .into_iter()
            .filter(|preset| preset.is_agent() && preset.worker_surface() == WorkerSurface::Chat)
            .collect();
        AgentPreset::sort_by_chat_preference(&mut agents);
        agents
    }

    pub(in crate::environment_shell) fn open_kraddie_editor(
        &mut self,
        id: Option<&KraddieId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = id
            .and_then(|id| self.kraddies.entry(id))
            .map(|entry| entry.kraddie.clone());
        let agents = self.kraddie_agents();
        let Some(harness) = existing
            .as_ref()
            .map(|kraddie| kraddie.harness)
            .or_else(|| agents.first().copied())
        else {
            return self.toast(NO_AGENT, cx);
        };
        let workspace = existing.as_ref().map_or_else(
            || self.active_kraddie_workspace(),
            |kraddie| kraddie.workspace.clone(),
        );
        let model = existing
            .as_ref()
            .and_then(|kraddie| kraddie.model.as_ref())
            .map_or_default(ToString::to_string);
        let name_text = existing
            .as_ref()
            .map_or_default(|kraddie| kraddie.name.to_string());
        let ideas = self.fresh_name_ideas(fresh_seed());
        self.kraddies.editor = Some(KraddieEditor {
            replacing: id.cloned(),
            harness,
            workspace,
            base: existing,
            ideas,
            shuffles: 0,
            refusal: None,
            refusals: 0,
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(NAME_PLACEHOLDER)
                    .default_value(name_text)
            }),
            model: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(MODEL_PLACEHOLDER)
                    .default_value(model)
            }),
        });
        self.list_kraddie_models(harness, cx);
        cx.notify();
    }

    fn list_kraddie_models(&mut self, harness: AgentPreset, cx: &mut Context<Self>) {
        if let AgentPreset::Acp(agent) = harness {
            self.list_acp_models(agent, cx);
        }
    }

    fn choose_kraddie_harness(&mut self, harness: AgentPreset, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.kraddies.editor else {
            return;
        };
        editor.harness = harness;
        self.list_kraddie_models(harness, cx);
        cx.notify();
    }

    fn pick_kraddie_model(&mut self, model: &ModelId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.kraddies.editor else {
            return;
        };
        editor.refusal = None;
        let field = editor.model.clone();
        field.update(cx, |input, cx| input.set_value(model.as_str(), window, cx));
        cx.notify();
    }

    pub(in crate::environment_shell) fn active_kraddie_workspace(
        &self,
    ) -> Option<KraddieWorkspace> {
        let group = self.active_group()?;
        let repo = self
            .git_at(&self.active, &group.root)
            .map_or_else(|| group.root.clone(), |git| git.repo_root.clone());
        Some(KraddieWorkspace {
            environment: self.active.clone(),
            repo,
        })
    }

    fn fresh_name_ideas(&self, seed: u64) -> Vec<&'static str> {
        let taken: Vec<KraddieId> = self
            .kraddies
            .entries
            .iter()
            .map(|entry| entry.kraddie.id())
            .collect();
        name_ideas(seed, NAME_IDEAS_SHOWN, &taken)
    }

    fn shuffle_name_ideas(&mut self, cx: &mut Context<Self>) {
        let ideas = self.fresh_name_ideas(fresh_seed());
        if let Some(editor) = &mut self.kraddies.editor {
            editor.ideas = ideas;
            editor.shuffles = editor.shuffles.wrapping_add(1);
        }
        cx.notify();
    }

    fn pick_name_idea(&mut self, idea: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.kraddies.editor else {
            return;
        };
        editor.refusal = None;
        let name = editor.name.clone();
        name.update(cx, |input, cx| input.set_value(idea, window, cx));
        cx.notify();
    }

    fn refuse_kraddie_editor(&mut self, reason: String, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.kraddies.editor {
            editor.refusal = Some(reason);
            editor.refusals += 1;
        }
        cx.notify();
    }

    fn close_kraddie_editor(&mut self, cx: &mut Context<Self>) {
        self.kraddies.editor = None;
        cx.notify();
    }

    fn save_kraddie_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.kraddies.editor else {
            return;
        };
        let read = |field: &Entity<InputState>| field.read(cx).value().to_string();
        let name = match read(&editor.name).parse::<KraddieName>() {
            Ok(name) => name,
            Err(error) => return self.refuse_kraddie_editor(error.to_string(), cx),
        };
        let taken = self.kraddies.entry(&name.id()).is_some()
            && editor.replacing.as_ref() != Some(&name.id());
        if taken {
            return self.refuse_kraddie_editor(super::panel::NAME_TAKEN.to_owned(), cx);
        }
        let model = match read_model(&read(&editor.model)) {
            Ok(model) => model,
            Err(error) => return self.refuse_kraddie_editor(error.to_owned(), cx),
        };
        let base = editor
            .base
            .clone()
            .unwrap_or_else(|| Kraddie::new(name.clone(), editor.harness));
        let kraddie = Kraddie {
            name,
            harness: editor.harness,
            model,
            workspace: editor.workspace.clone(),
            ..base
        };
        let replacing = editor.replacing.clone();
        self.kraddies.editor = None;
        self.save_kraddie(kraddie, replacing.as_ref(), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_model_means_the_agents_default() {
        assert_eq!(read_model("  "), Ok(None));
    }
}
