use gpui_kit::{AppContext as _, Context, Task};
use krad_chat::{opencode_models, ModelOffer, OpencodeError, OpencodeServer};
use krad_domain::{AgentPreset, AgentState, EnvironmentName, ResumeId, TerminalId};
use std::collections::HashMap;
use std::sync::Arc;

use super::EnvironmentShell;

#[derive(Default)]
pub(super) enum OpencodeLink {
    #[default]
    Absent,
    Starting,
    Ready {
        server: Arc<OpencodeServer>,
        models: Vec<ModelOffer>,
    },
    Failed(String),
}

#[derive(Default)]
pub(super) struct Opencode {
    pub(super) link: OpencodeLink,
    unwatched: Vec<(EnvironmentName, TerminalId, ResumeId)>,
    pub(super) waiting_histories: Vec<(EnvironmentName, TerminalId, ResumeId)>,
    watches: HashMap<(EnvironmentName, TerminalId), Task<()>>,
}

impl EnvironmentShell {
    pub(super) fn ensure_opencode(&mut self, cx: &mut Context<Self>) {
        let installed = self.native_agents.contains(&AgentPreset::Opencode);
        if !installed || !matches!(self.opencode.link, OpencodeLink::Absent) {
            return;
        }
        let Ok((runner, cwd)) = self.harness_runner(AgentPreset::Opencode, None) else {
            return;
        };
        self.opencode.link = OpencodeLink::Starting;
        let started = cx.background_spawn(async move {
            let server = OpencodeServer::start(&runner, &cwd, &[])?;
            let models = opencode_models(&server, None)?;
            Ok::<_, OpencodeError>((server, models))
        });
        cx.spawn(async move |this, cx| {
            let started = started.await;
            this.update(cx, |shell, cx| {
                shell.opencode.link = match started {
                    Ok((server, models)) => OpencodeLink::Ready { server, models },
                    Err(error) => OpencodeLink::Failed(error.to_string()),
                };
                shell.load_waiting_histories(cx);
                shell.flush_chats_of(AgentPreset::Opencode, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) const fn opencode_server(&self) -> Option<&Arc<OpencodeServer>> {
        match &self.opencode.link {
            OpencodeLink::Ready { server, .. } => Some(server),
            OpencodeLink::Absent | OpencodeLink::Starting | OpencodeLink::Failed(_) => None,
        }
    }

    pub(super) fn opencode_models(&self) -> &[ModelOffer] {
        match &self.opencode.link {
            OpencodeLink::Ready { models, .. } => models,
            OpencodeLink::Absent | OpencodeLink::Starting | OpencodeLink::Failed(_) => &[],
        }
    }

    pub(super) fn note_opencode_terminal(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        resume: ResumeId,
    ) {
        self.opencode.unwatched.push((env.clone(), id, resume));
    }

    pub(super) fn watch_new_opencode_terminals(&mut self, cx: &mut Context<Self>) {
        for (env, id, resume) in std::mem::take(&mut self.opencode.unwatched) {
            self.watch_opencode_terminal(&env, id, &resume, cx);
        }
    }

    fn watch_opencode_terminal(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        resume: &ResumeId,
        cx: &mut Context<Self>,
    ) {
        let Some(server) = self.opencode_server().cloned() else {
            return;
        };
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        krad_chat::watch_opencode_session(&server, resume, move |state| {
            sender.unbounded_send(state).unwrap_or_default();
        });
        let owner = env.clone();
        let task = Self::listen(receiver, cx, move |shell, state: AgentState, _| {
            shell.apply_opencode_state(&owner, id, state);
        });
        self.opencode.watches.insert((env.clone(), id), task);
    }

    fn apply_opencode_state(&mut self, env: &EnvironmentName, id: TerminalId, state: AgentState) {
        let Some(session) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.sessions.get_mut(&id))
        else {
            return;
        };
        session.hooked = true;
        self.set_agent_state(env, id, state);
    }

    pub(super) fn forget_opencode_terminal(&mut self, env: &EnvironmentName, id: TerminalId) {
        self.opencode.watches.remove(&(env.clone(), id));
    }
}
