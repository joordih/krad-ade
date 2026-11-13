use gpui_kit::Context;
use krad_chat::{OpencodeServer, Runner};
use krad_domain::{AgentPreset, AgentState, EnvironmentName, Host, ResumeId, TerminalId};
use krad_host::HostLink;
use krad_protocol::{OpencodeShown, OpencodeSnapshot, OpencodeStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::EnvironmentShell;

#[derive(Default)]
pub(super) struct OpencodeMirror(HashMap<Host, OpencodeStatus>);

impl OpencodeMirror {
    pub(super) fn replace(&mut self, snapshot: OpencodeSnapshot) {
        self.0 = snapshot
            .servers
            .into_iter()
            .map(|shown| (shown.host, shown.status))
            .collect();
    }

    fn apply(&mut self, shown: OpencodeShown) {
        match shown.status {
            OpencodeStatus::Stopped => self.0.remove(&shown.host),
            status => self.0.insert(shown.host, status),
        };
    }

    pub(super) fn of(&self, host: &Host) -> Option<&OpencodeStatus> {
        self.0.get(host)
    }
}

#[derive(Default)]
pub(super) struct Opencode {
    unwatched: Vec<(EnvironmentName, TerminalId, ResumeId)>,
    pub(super) waiting_histories: Vec<(EnvironmentName, TerminalId, ResumeId)>,
    watches: HashMap<(EnvironmentName, TerminalId), gpui_kit::Task<()>>,
}

impl EnvironmentShell {
    pub(super) fn ensure_opencode(&mut self) {
        if !self.native_agents.contains(&AgentPreset::Opencode) {
            return;
        }
        if self.runtime.view.opencode.of(&Host::Local).is_some() {
            return;
        }
        let Ok((runner, cwd)) = self.harness_runner(&HostLink::Local, AgentPreset::Opencode, None)
        else {
            return;
        };
        self.ensure_host_opencode(runner, cwd);
    }

    pub(super) fn ensure_host_opencode(&self, runner: Runner, cwd: PathBuf) {
        let host = runner.link().host();
        if self.runtime.view.opencode.of(&host).is_none() {
            self.runtime.handle.start_opencode(host, runner, cwd);
        }
    }

    pub(super) fn opencode_heard(&mut self, shown: OpencodeShown, cx: &mut Context<Self>) {
        let ready = shown.status == OpencodeStatus::Ready;
        self.runtime.view.opencode.apply(shown);
        if ready {
            self.watch_new_opencode_terminals(cx);
            self.place_usage();
        }
    }

    pub(super) fn refresh_opencode_models(&self) {
        self.runtime.handle.refresh_opencode_models(Host::Local);
    }

    pub(super) fn opencode_server_of(&self, env: &EnvironmentName) -> Option<Arc<OpencodeServer>> {
        let host = &self.envs.get(env)?.data.host;
        self.runtime.handle.opencode_server(host)
    }

    pub(super) fn opencode_server(&self) -> Option<Arc<OpencodeServer>> {
        self.runtime.handle.opencode_server(&Host::Local)
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
        let Some(server) = self.opencode_server() else {
            return;
        };
        for (env, id, resume) in std::mem::take(&mut self.opencode.unwatched) {
            self.watch_opencode_terminal(&server, (&env, id), &resume, cx);
        }
    }

    fn watch_opencode_terminal(
        &mut self,
        server: &OpencodeServer,
        (env, id): (&EnvironmentName, TerminalId),
        resume: &ResumeId,
        cx: &mut Context<Self>,
    ) {
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        krad_chat::watch_opencode_session(server, resume, move |state| {
            sender.unbounded_send(state).unwrap_or_default();
        });
        let owner = env.clone();
        let task = Self::listen(receiver, cx, move |shell, state: AgentState, _| {
            shell.apply_opencode_state(&owner, id, state);
        });
        self.opencode.watches.insert((env.clone(), id), task);
    }

    fn apply_opencode_state(&mut self, env: &EnvironmentName, id: TerminalId, state: AgentState) {
        let runs = self
            .envs
            .get(env)
            .is_some_and(|live| live.sessions.contains_key(&id));
        if !runs {
            return;
        }
        self.mark_terminal_hooked(env, id);
        self.set_agent_state(env, id, state);
    }

    pub(super) fn forget_opencode_terminal(&mut self, env: &EnvironmentName, id: TerminalId) {
        self.opencode.watches.remove(&(env.clone(), id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(status: OpencodeStatus) -> OpencodeShown {
        OpencodeShown {
            host: Host::Local,
            status,
        }
    }

    #[test]
    fn the_mirror_follows_the_runtime_servers_and_forgets_a_stopped_one() {
        let mut mirror = OpencodeMirror::default();
        mirror.apply(shown(OpencodeStatus::Starting));
        mirror.apply(shown(OpencodeStatus::Ready));
        assert_eq!(mirror.of(&Host::Local), Some(&OpencodeStatus::Ready));
        mirror.apply(shown(OpencodeStatus::Stopped));
        assert_eq!(mirror.of(&Host::Local), None);
        mirror.replace(OpencodeSnapshot {
            servers: vec![shown(OpencodeStatus::Starting)],
        });
        assert_eq!(mirror.of(&Host::Local), Some(&OpencodeStatus::Starting));
    }
}
