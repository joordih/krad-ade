use gpui_kit::{AppContext as _, Context, Task};
use krad_chat::Runner;
use krad_chat::{opencode_models, ModelOffer, OpencodeError, OpencodeServer};
use krad_domain::{AgentPreset, AgentState, EnvironmentName, Host, ResumeId, TerminalId};
use krad_host::HostLink;
use std::collections::HashMap;
use std::path::PathBuf;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Refresh {
    #[default]
    Idle,
    Running,
    Queued,
}

#[derive(Default)]
pub(super) struct Opencode {
    pub(super) link: OpencodeLink,
    pub(super) hosts: HashMap<Host, OpencodeLink>,
    refresh: Refresh,
    catalog_watch: Option<Task<()>>,
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
        let Ok((runner, cwd)) = self.harness_runner(&HostLink::Local, AgentPreset::Opencode, None)
        else {
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
                    Ok((server, models)) => {
                        shell.watch_opencode_catalog(&server, cx);
                        OpencodeLink::Ready { server, models }
                    }
                    Err(error) => OpencodeLink::Failed(error.to_string()),
                };
                shell.load_waiting_histories(cx);
                shell.flush_chats_of(AgentPreset::Opencode, cx);
                shell.read_usage_on_time(cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn watch_opencode_catalog(&mut self, server: &OpencodeServer, cx: &mut Context<Self>) {
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        server.watch_catalog(move || {
            sender.unbounded_send(()).unwrap_or_default();
        });
        let task = Self::listen(receiver, cx, |shell, (), cx| {
            shell.refresh_opencode_models(cx);
        });
        self.opencode.catalog_watch = Some(task);
    }

    pub(super) fn refresh_opencode_models(&mut self, cx: &mut Context<Self>) {
        let Some(server) = self.opencode_server().cloned() else {
            return;
        };
        if self.opencode.refresh != Refresh::Idle {
            self.opencode.refresh = Refresh::Queued;
            return;
        }
        self.opencode.refresh = Refresh::Running;
        let listed = cx.background_spawn(async move { opencode_models(&server, None) });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            this.update(cx, |shell, cx| shell.opencode_models_listed(listed, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn opencode_models_listed(
        &mut self,
        listed: Result<Vec<ModelOffer>, OpencodeError>,
        cx: &mut Context<Self>,
    ) {
        let queued = self.opencode.refresh == Refresh::Queued;
        self.opencode.refresh = Refresh::Idle;
        match listed {
            Ok(fresh) => self.keep_opencode_models(fresh),
            Err(error) => self.toast(&error.to_string(), cx),
        }
        if queued {
            self.refresh_opencode_models(cx);
        }
        cx.notify();
    }

    fn keep_opencode_models(&mut self, fresh: Vec<ModelOffer>) {
        if fresh.is_empty() {
            return;
        }
        if let OpencodeLink::Ready { models, .. } = &mut self.opencode.link {
            *models = fresh;
        }
    }

    pub(super) fn ensure_host_opencode(
        &mut self,
        runner: Runner,
        cwd: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let host = runner.link().host();
        if self.opencode.hosts.contains_key(&host) {
            return;
        }
        self.opencode
            .hosts
            .insert(host.clone(), OpencodeLink::Starting);
        let started = cx.background_spawn(async move {
            let server = OpencodeServer::start(&runner, &cwd, &[])?;
            let models = opencode_models(&server, None)?;
            Ok::<_, OpencodeError>((server, models))
        });
        cx.spawn(async move |this, cx| {
            let started = started.await;
            this.update(cx, |shell, cx| {
                let link = match started {
                    Ok((server, models)) => OpencodeLink::Ready { server, models },
                    Err(error) => OpencodeLink::Failed(error.to_string()),
                };
                shell.opencode.hosts.insert(host, link);
                shell.load_waiting_histories(cx);
                shell.flush_chats_of(AgentPreset::Opencode, cx);
                shell.read_usage_on_time(cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn start_host_opencode_for(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let link = self.env_link(env);
        if link.is_local() {
            return;
        }
        let root = self
            .envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        match self.harness_runner(&link, AgentPreset::Opencode, root.as_deref()) {
            Ok((runner, cwd)) => self.ensure_host_opencode(runner, cwd, cx),
            Err(error) => tracing::debug!(%error, "opencode cannot start on the host yet"),
        }
    }

    pub(super) fn opencode_link_of(&self, env: &EnvironmentName) -> Option<&OpencodeLink> {
        let host = &self.envs.get(env)?.data.host;
        if host.is_local() {
            return Some(&self.opencode.link);
        }
        self.opencode.hosts.get(host)
    }

    pub(super) fn opencode_server_of(&self, env: &EnvironmentName) -> Option<&Arc<OpencodeServer>> {
        match self.opencode_link_of(env)? {
            OpencodeLink::Ready { server, .. } => Some(server),
            OpencodeLink::Absent | OpencodeLink::Starting | OpencodeLink::Failed(_) => None,
        }
    }

    pub(super) const fn opencode_server(&self) -> Option<&Arc<OpencodeServer>> {
        match &self.opencode.link {
            OpencodeLink::Ready { server, .. } => Some(server),
            OpencodeLink::Absent | OpencodeLink::Starting | OpencodeLink::Failed(_) => None,
        }
    }

    pub(super) fn opencode_models_on(&self, host: &Host) -> &[ModelOffer] {
        let link = if host.is_local() {
            Some(&self.opencode.link)
        } else {
            self.opencode.hosts.get(host)
        };
        match link {
            Some(OpencodeLink::Ready { models, .. }) => models,
            Some(OpencodeLink::Absent | OpencodeLink::Starting | OpencodeLink::Failed(_))
            | None => &[],
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
