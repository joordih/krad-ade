use krad_domain::{AgentState, EnvironmentName, ProcessRun, TerminalId, ToastKind};
use std::time::Instant;

use super::super::live_terminal::LiveTerminal;
use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn terminal_classified(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        state: AgentState,
    ) -> bool {
        let unhooked = self
            .envs
            .get(env)
            .and_then(|live| live.sessions.get(&id))
            .is_some_and(|terminal| !terminal.hooked());
        unhooked && self.set_agent_state(env, id, state)
    }

    pub(in crate::environment_shell) fn terminal_ended(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        run: ProcessRun,
    ) -> bool {
        let first = self
            .envs
            .get_mut(env)
            .and_then(|live| live.sessions.get_mut(&id))
            .is_some_and(LiveTerminal::hear_end);
        if !first {
            return false;
        }
        match run {
            ProcessRun::Running => false,
            ProcessRun::Exited { code } => self.apply_exit(env, id, code),
            ProcessRun::Failed { error } => self.apply_spawn_failed(id, &error),
        }
    }

    fn apply_exit(&mut self, env: &EnvironmentName, id: TerminalId, code: Option<i32>) -> bool {
        let signed_in = self.sign_in_ended(env, id, code);
        self.acp_sign_in_ended(env, id);
        self.runner_helper_exited(env, id);
        let Some(live) = self.envs.get_mut(env) else {
            return signed_in;
        };
        if !live.sessions.contains_key(&id) {
            return signed_in;
        }
        if let Some(node) = live.data.nodes.iter_mut().find(|node| node.id == id) {
            node.state = AgentState::Idle;
        }
        self.publish_agent(env, id, None);
        let raw = id.as_u64();
        match code {
            Some(code) => self.push_toast(
                ToastKind::Info,
                format!("terminal #{raw} exited (code {code})"),
            ),
            None => self.push_toast(ToastKind::Info, format!("terminal #{raw} exited")),
        }
        self.store();
        true
    }

    fn apply_spawn_failed(&mut self, id: TerminalId, error: &str) -> bool {
        let raw = id.as_u64();
        tracing::debug!(terminal = raw, %error, "a terminal failed");
        self.push_toast(
            ToastKind::Error,
            format!("terminal #{raw} failed to spawn: {error}"),
        );
        true
    }

    pub(in crate::environment_shell) fn apply_tick(&mut self) -> bool {
        let now = Instant::now();
        let changed = self.expire_toasts(now);
        self.service_agent_bus(now);
        self.reconcile_agent_feed();
        self.hear_runner_sign_ins();
        changed
    }
}
