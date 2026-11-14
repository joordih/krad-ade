use gpui_kit::App;
mod launch;
mod terminal_events;

pub(super) use launch::Launch;

use krad_domain::{
    agent_event, AgentState, EnvironmentName, LaunchIntent, NodeField, ResumeId, TerminalId,
    ToastKind,
};
use krad_protocol::{Command, TerminalCommand};
use krad_runtime::{HostShell, TerminalLaunch};
use std::path::PathBuf;
use std::time::Instant;

use super::live_terminal::{terminal_ref, LiveTerminal};
use super::EnvironmentShell;

const HOST_NOT_READY_TEXT: &str = "Connect to the environment's host first";

impl EnvironmentShell {
    pub(super) fn spawn_missing_sessions(&mut self, env: &EnvironmentName, cx: &App) -> usize {
        if !self.host_ready(env, cx) {
            return 0;
        }
        let missing = self.envs.get(env).map_or(0, |live| {
            live.data
                .relaunches(|id| live.sessions.contains_key(&id))
                .len()
        });
        self.runtime
            .client
            .post(Command::Terminal(TerminalCommand::OpenMissing {
                environment: env.clone(),
            }));
        missing
    }

    fn launch_in_runtime(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        intent: LaunchIntent,
        cwd: Option<PathBuf>,
    ) {
        self.save_view();
        self.runtime
            .client
            .post(Command::Terminal(TerminalCommand::Open {
                terminal: terminal_ref(env, id),
                intent,
                cwd,
            }));
    }

    pub(super) fn start_session(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        launch: Launch,
        cwd: Option<PathBuf>,
        prefill: &[String],
        cx: &mut App,
    ) {
        match LaunchIntent::try_from(launch) {
            Ok(intent) => self.launch_in_runtime(env, id, intent, cwd),
            Err(launch) => self.spawn_session(env, id, (launch, cwd, prefill), cx),
        }
    }

    fn spawn_session(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        (launch, cwd, prefill): (Launch, Option<PathBuf>, &[String]),
        cx: &mut App,
    ) {
        let Launch::Command {
            program,
            args,
            vars,
        } = launch
        else {
            return;
        };
        if !self.host_ready(env, cx) {
            self.push_toast(ToastKind::Error, HOST_NOT_READY_TEXT.to_owned());
            return;
        }
        let (wsl, _) = self.launch_site(env, cwd.as_deref());
        let place =
            self.runtime
                .handle
                .host_place(&self.env_host(env), HostShell::Given, wsl.is_some());
        let launch = TerminalLaunch {
            program,
            args,
            cwd,
            vars: self.launch_vars(env, id, vars, cx),
            place,
            harness: None,
            prefill: prefill.to_vec(),
            typed: None,
            tap: None,
        };
        let view = self
            .terminals()
            .open_terminal(terminal_ref(env, id), launch);
        self.keep_live_session(env, id, LiveTerminal::new(view, None), cx);
    }

    pub(super) fn keep_live_session(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        session: LiveTerminal,
        cx: &mut App,
    ) {
        let harness_changed = self.envs.get(env).is_some_and(|live| {
            live.data
                .nodes
                .iter()
                .any(|node| node.id == id && node.harness != session.harness)
        });
        if harness_changed {
            self.set_node(env, id, NodeField::Harness(session.harness), cx);
        }
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.sessions.insert(id, session);
        if live.focused.is_none() {
            live.focused = Some(id);
        }
    }

    fn keep_opencode_resume(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        resume: ResumeId,
        cx: &mut App,
    ) {
        self.set_node(env, id, NodeField::Resume(Some(resume)), cx);
    }

    pub(super) fn set_agent_state(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        state: AgentState,
        cx: &mut App,
    ) -> bool {
        let Some(node) = self
            .envs
            .get(env)
            .and_then(|live| live.data.nodes.iter().find(|node| node.id == id))
        else {
            return false;
        };
        if node.state == state {
            return false;
        }
        let previous = node.state;
        let event = agent_event(id, node.harness, previous, state);
        let group = node.group;
        let waiting = previous == AgentState::Working && state == AgentState::WaitingForInput;
        self.set_node(env, id, NodeField::State(state), cx);
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        if state == AgentState::Working {
            live.working_since.insert(id, Instant::now());
        } else {
            live.working_since.remove(&id);
        }
        if waiting {
            let raw = id.as_u64();
            let name = env.as_str().to_owned();
            self.push_toast(
                ToastKind::Info,
                format!("agent #{raw} waits for input ({name})"),
            );
            if *env != self.active {
                self.unseen.insert(env.clone());
            }
        }
        self.observe_attention(env, id, previous, state);
        self.publish_agent(env, id, None);
        if let Some(event) = event {
            self.publish_event(env, group, event);
        }
        true
    }
}
