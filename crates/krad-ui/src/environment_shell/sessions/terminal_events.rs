use krad_domain::{AgentState, EnvironmentName, NodeField, ProcessRun, TerminalId, ToastKind};
use krad_protocol::{AgentRef, LaunchedAs, TerminalLaunched};
use std::time::Instant;

use super::super::live_terminal::LiveTerminal;
use super::super::shell_site::ShellSite;
use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn terminal_launched(&mut self, launched: TerminalLaunched) {
        let TerminalLaunched {
            terminal,
            harness,
            launched_as,
            resume,
        } = launched;
        let Some(view) = self.runtime.handle.take_launched_view(&terminal) else {
            return;
        };
        let (env, id) = (terminal.environment, terminal.terminal);
        if !self.envs.contains_key(&env) {
            return;
        }
        self.keep_live_session(&env, id, LiveTerminal::new(view, harness));
        if launched_as == LaunchedAs::Shell {
            let site = ShellSite::of(&self.env_link(&env), false);
            self.activate_toolchains(&env, id, site);
        }
        if let Some(resume) = resume {
            self.keep_opencode_resume(&env, id, resume);
        }
    }

    pub(in crate::environment_shell) fn adopt_launched_view(&mut self, terminal: &AgentRef) {
        let Some(view) = self.runtime.handle.take_launched_view(terminal) else {
            return;
        };
        let (env, id) = (&terminal.environment, terminal.terminal);
        let harness = self.envs.get(env).and_then(|live| {
            live.data
                .nodes
                .iter()
                .find(|node| node.id == id)
                .and_then(|node| node.harness)
        });
        self.keep_live_session(env, id, LiveTerminal::new(view, harness));
    }

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
        if self.sign_in_terminal_of(env) == Some(id) {
            return true;
        }
        self.acp_sign_in_ended(env, id);
        self.runner_helper_exited(env, id);
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        if !live.sessions.contains_key(&id) {
            return false;
        }
        self.set_node(env, id, NodeField::State(AgentState::Idle));
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
        self.service_agent_bus();
        self.reconcile_agent_feed();
        self.hear_runner_sign_ins();
        changed
    }
}
