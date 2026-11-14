use gpui_kit::App;
use krad_domain::{AgentPreset, AgentState, EnvironmentName, Host, TerminalId};
use krad_protocol::{
    Command, HarnessSpot, OpencodeCommand, OpencodeShown, OpencodeSnapshot, OpencodeStatus,
};
use std::collections::HashMap;

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

    pub(super) fn apply(&mut self, shown: OpencodeShown) {
        match shown.status {
            OpencodeStatus::Stopped => self.0.remove(&shown.host),
            status => self.0.insert(shown.host, status),
        };
    }

    pub(super) fn of(&self, host: &Host) -> Option<&OpencodeStatus> {
        self.0.get(host)
    }
}

impl EnvironmentShell {
    pub(super) fn ensure_opencode(&mut self, cx: &App) {
        let known = self
            .runtime
            .view
            .read(cx)
            .opencode
            .of(&Host::Local)
            .is_some();
        if known || !self.native_agents.contains(&AgentPreset::Opencode) {
            return;
        }
        self.runtime
            .client
            .post(Command::Opencode(OpencodeCommand::Start {
                spot: HarnessSpot::default(),
            }));
    }

    pub(super) fn opencode_ready(&mut self) {
        self.place_usage();
    }

    pub(super) fn refresh_opencode_models(&self) {
        self.runtime
            .client
            .post(Command::Opencode(OpencodeCommand::Refresh {
                host: Host::Local,
            }));
    }

    pub(super) fn apply_opencode_state(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        state: AgentState,
    ) {
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
