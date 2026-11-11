use gpui_kit::Context;
use krad_domain::{
    snap_effort, AgentPreset, ChatChoice, Effort, EffortSnap, EnvironmentName, ModelId, TerminalId,
    TerminalNode,
};

use super::super::EnvironmentShell;
use super::facts::model_label;
use super::structured::StructuredChat;
use super::{Backend, SurfaceCommand};

const UNAVAILABLE_MODEL_NOTICE: &str =
    "is no longer offered here, so this message uses the default model";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chosen<'a> {
    Model(&'a ModelId),
    Effort(Effort),
}

impl<'a> Chosen<'a> {
    const fn of(command: &'a SurfaceCommand) -> Option<Self> {
        match command {
            SurfaceCommand::Model(_, model) => Some(Self::Model(model)),
            SurfaceCommand::Effort(effort) => Some(Self::Effort(*effort)),
            SurfaceCommand::Stop | SurfaceCommand::ToggleThinking => None,
        }
    }

    fn apply(self, choice: &mut ChatChoice) {
        match self {
            Self::Model(model) => choice.model = Some(model.clone()),
            Self::Effort(effort) => choice.effort = Some(effort),
        }
    }
}

fn node_mut<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut TerminalNode> {
    shell
        .envs
        .get_mut(env)?
        .data
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
}

fn structured_mut<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut StructuredChat> {
    match &mut shell.envs.get_mut(env)?.chats.get_mut(&id)?.backend {
        Backend::Structured(structured) => Some(structured),
        Backend::Transcript(_) => None,
    }
}

impl EnvironmentShell {
    fn chat_harness_of(&self, env: &EnvironmentName, id: TerminalId) -> Option<AgentPreset> {
        let live = self.envs.get(env)?;
        match &live.chats.get(&id)?.backend {
            Backend::Structured(structured) => Some(structured.harness),
            Backend::Transcript(_) => live
                .data
                .nodes
                .iter()
                .find(|node| node.id == id)
                .and_then(|node| node.harness),
        }
    }

    pub(super) fn remember_choice(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        command: &SurfaceCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(chosen) = Chosen::of(command) else {
            return;
        };
        let harness = match command {
            SurfaceCommand::Model(harness, _) => Some(*harness),
            _ => self.chat_harness_of(env, id),
        };
        if let Some(node) = node_mut(self, env, id) {
            chosen.apply(&mut node.choice);
        }
        if let Some(harness) = harness {
            chosen.apply(self.settings.chat_defaults.entry(harness).or_default());
            self.save_settings(cx);
        }
        self.store();
    }

    fn model_efforts(&self, harness: AgentPreset, model: &ModelId) -> Option<Vec<Effort>> {
        self.harness_offers(harness)?
            .iter()
            .find(|offer| offer.answers_to(model.as_str()))
            .map(|offer| offer.efforts.clone())
    }

    pub(super) fn fit_effort_to_model(&mut self, env: &EnvironmentName, id: TerminalId) {
        let Some((harness, Some(model))) = structured_mut(self, env, id)
            .map(|structured| (structured.harness, structured.chosen_model.clone()))
        else {
            return;
        };
        let Some(allowed) = self.model_efforts(harness, &model) else {
            return;
        };
        let remembered = self
            .settings
            .chat_defaults
            .get(&harness)
            .and_then(|choice| choice.effort)
            .filter(|effort| allowed.contains(effort));
        let Some(structured) = structured_mut(self, env, id) else {
            return;
        };
        let fitted = match (structured.effort, snap_effort(structured.effort, &allowed)) {
            (None, _) => remembered,
            (Some(_), EffortSnap::Kept) => {
                structured.effort_notice = None;
                return;
            }
            (Some(_), snap @ EffortSnap::Moved { to, .. }) => {
                structured.command(SurfaceCommand::Effort(to));
                structured.effort_notice = Some(snap);
                Some(to)
            }
            (Some(_), snap @ EffortSnap::Dropped(_)) => {
                structured.effort = None;
                structured.effort_notice = Some(snap);
                None
            }
        };
        if let Some(effort) = fitted.filter(|_| structured.effort.is_none()) {
            structured.command(SurfaceCommand::Effort(effort));
        }
        if let Some(node) = node_mut(self, env, id) {
            node.choice.effort = fitted;
        }
    }

    pub(super) fn restored_choice(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
        explicit: ChatChoice,
    ) -> ChatChoice {
        let kept = self
            .envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .map_or_default(|node| node.choice.clone());
        let remembered = self
            .settings
            .chat_defaults
            .get(&harness)
            .cloned()
            .unwrap_or_default();
        let preferred = ChatChoice {
            model: self.new_chat_model(harness, remembered.model.as_ref()),
            effort: remembered.effort,
        };
        explicit.or(&kept).or(&preferred)
    }

    fn new_chat_model(&self, harness: AgentPreset, last_used: Option<&ModelId>) -> Option<ModelId> {
        let offers = self.harness_offers(harness);
        let offered = |model: &ModelId| {
            offers.is_some_and(|offers| offers.iter().any(|offer| offer.answers_to(model.as_str())))
        };
        let listed: Option<&dyn Fn(&ModelId) -> bool> = if offers.is_some() {
            Some(&offered)
        } else {
            None
        };
        self.settings
            .model_preferences
            .new_chat_model(harness, last_used, listed)
    }

    pub(in crate::environment_shell) fn forget_unoffered_models(&mut self, harness: AgentPreset) {
        let Some(offers) = self.harness_offers(harness) else {
            return;
        };
        let offered = |model: &ModelId| offers.iter().any(|offer| offer.answers_to(model.as_str()));
        let env = self.active.clone();
        let stale: Vec<TerminalId> = self.envs.get(&env).map_or_default(|live| {
            live.chats
                .iter()
                .filter_map(|(id, chat)| match &chat.backend {
                    Backend::Structured(structured)
                        if structured.harness == harness
                            && chat.messages.is_empty()
                            && structured.is_fresh()
                            && structured
                                .chosen_model
                                .as_ref()
                                .is_some_and(|model| !offered(model)) =>
                    {
                        Some(*id)
                    }
                    Backend::Structured(_) | Backend::Transcript(_) => None,
                })
                .collect()
        });
        for id in stale {
            if let Some(structured) = structured_mut(self, &env, id) {
                structured.chosen_model = None;
            }
            if let Some(node) = node_mut(self, &env, id) {
                node.choice.model = None;
            }
        }
    }

    pub(super) fn offered_choice(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
    ) -> (Option<ModelId>, Option<String>) {
        let chosen = structured_mut(self, env, id).and_then(|chat| chat.chosen_model.clone());
        let (model, notice) = self.offered_model(harness, chosen);
        if notice.is_some() {
            if let Some(structured) = structured_mut(self, env, id) {
                structured.chosen_model = None;
            }
            if let Some(node) = node_mut(self, env, id) {
                node.choice.model = None;
            }
        }
        (model, notice)
    }

    pub(super) fn keep_choice(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        choice: ChatChoice,
    ) {
        if let Some(node) = node_mut(self, env, id) {
            node.choice = choice;
        }
    }

    fn offered_model(
        &self,
        harness: AgentPreset,
        chosen: Option<ModelId>,
    ) -> (Option<ModelId>, Option<String>) {
        let Some(model) = chosen else {
            return (None, None);
        };
        let gone = self
            .harness_offers(harness)
            .is_some_and(|offers| !offers.iter().any(|offer| offer.answers_to(model.as_str())));
        if gone {
            let notice = format!("{} {UNAVAILABLE_MODEL_NOTICE}", model_label(model.as_str()));
            return (None, Some(notice));
        }
        (Some(model), None)
    }
}
