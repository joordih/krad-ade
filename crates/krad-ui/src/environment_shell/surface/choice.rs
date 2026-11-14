use gpui_kit::{App, Context};
use krad_domain::{
    snap_effort, AgentPreset, ChatChoice, Effort, EffortSnap, EnvironmentName, ModelId, NodeField,
    TerminalId,
};

use super::super::EnvironmentShell;

use super::structured::StructuredChat;
use super::{Backend, SurfaceCommand};

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

fn edit_choice(
    shell: &mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
    change: impl FnOnce(&mut ChatChoice),
) {
    let Some(before) = shell
        .envs
        .get(env)
        .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
        .map(|node| node.choice.clone())
    else {
        return;
    };
    let mut choice = before.clone();
    change(&mut choice);
    if choice != before {
        shell.set_node(env, id, NodeField::Choice(choice));
    }
}

fn structured_mut<'a>(
    shell: &'a mut EnvironmentShell,
    env: &EnvironmentName,
    id: TerminalId,
) -> Option<&'a mut StructuredChat> {
    match &mut shell.envs.get_mut(env)?.chats.get_mut(&id)?.chat.backend {
        Backend::Structured(structured) => Some(structured),
        Backend::Transcript(_) => None,
    }
}

impl EnvironmentShell {
    fn chat_harness_of(&self, env: &EnvironmentName, id: TerminalId) -> Option<AgentPreset> {
        let live = self.envs.get(env)?;
        match &live.chats.get(&id)?.chat.backend {
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
        edit_choice(self, env, id, |choice| chosen.apply(choice));
        if let Some(harness) = harness {
            self.edit_settings(cx, |settings| {
                chosen.apply(settings.chat_defaults.entry(harness).or_default());
            });
        }
        self.store();
    }

    fn model_efforts(
        &self,
        harness: AgentPreset,
        model: &ModelId,
        cx: &App,
    ) -> Option<Vec<Effort>> {
        self.harness_offers(harness, cx)?
            .iter()
            .find(|offer| offer.answers_to(model.as_str()))
            .map(|offer| offer.efforts.clone())
    }

    pub(super) fn fit_effort_to_model(&mut self, env: &EnvironmentName, id: TerminalId, cx: &App) {
        let Some((harness, Some(model))) = structured_mut(self, env, id)
            .map(|structured| (structured.harness, structured.chosen_model.clone()))
        else {
            return;
        };
        let Some(allowed) = self.model_efforts(harness, &model, cx) else {
            return;
        };
        let remembered = self
            .runtime
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
        edit_choice(self, env, id, |choice| choice.effort = fitted);
    }

    pub(super) fn restored_choice(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        harness: AgentPreset,
        explicit: ChatChoice,
        cx: &App,
    ) -> ChatChoice {
        let kept = self
            .envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
            .map_or_default(|node| node.choice.clone());
        let remembered = self
            .runtime
            .settings
            .chat_defaults
            .get(&harness)
            .cloned()
            .unwrap_or_default();
        let preferred = ChatChoice {
            model: self.new_chat_model(harness, remembered.model.as_ref(), cx),
            effort: remembered.effort,
        };
        explicit.or(&kept).or(&preferred)
    }

    fn new_chat_model(
        &self,
        harness: AgentPreset,
        last_used: Option<&ModelId>,
        cx: &App,
    ) -> Option<ModelId> {
        let offers = self.harness_offers(harness, cx);
        let offered = |model: &ModelId| {
            offers.is_some_and(|offers| offers.iter().any(|offer| offer.answers_to(model.as_str())))
        };
        let listed: Option<&dyn Fn(&ModelId) -> bool> = if offers.is_some() {
            Some(&offered)
        } else {
            None
        };
        self.runtime
            .settings
            .model_preferences
            .new_chat_model(harness, last_used, listed)
    }

    pub(in crate::environment_shell) fn forget_unoffered_models(
        &mut self,
        harness: AgentPreset,
        cx: &App,
    ) {
        let Some(offers) = self.harness_offers(harness, cx) else {
            return;
        };
        let offered = |model: &ModelId| offers.iter().any(|offer| offer.answers_to(model.as_str()));
        let env = self.active.clone();
        let stale: Vec<TerminalId> = self.envs.get(&env).map_or_default(|live| {
            live.chats
                .iter()
                .filter_map(|(id, chat)| match &chat.chat.backend {
                    Backend::Structured(structured)
                        if structured.harness == harness
                            && chat.chat.messages.is_empty()
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
            edit_choice(self, &env, id, |choice| choice.model = None);
        }
    }

    pub(super) fn keep_choice(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        choice: ChatChoice,
    ) {
        edit_choice(self, env, id, |kept| *kept = choice);
    }
}
