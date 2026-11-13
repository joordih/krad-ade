mod launch;
mod terminal_events;
mod typed_line;

pub(super) use launch::Launch;

use krad_domain::{
    agent_event, AgentState, EnvironmentName, LaunchIntent, NodeField, Relaunch, ResumeId,
    TerminalId, ToastKind,
};
use krad_protocol::{Command, TerminalCommand};
use krad_runtime::{AgentRole, TerminalLaunch, TerminalPlace};
use std::path::PathBuf;
use std::time::Instant;

use self::launch::{LaunchShell, ResolvedLaunch};
use self::typed_line::{type_attached_line, TypedLine};
use super::cloud::CloudShell;
use super::live_terminal::{terminal_ref, LiveTerminal};
use super::shell_site::ShellSite;
use super::EnvironmentShell;

type CloudOpener = (krad_session::PtyOpener, bool);

const HOST_NOT_READY_TEXT: &str = "Connect to the environment's host first";

pub(super) struct DeferredLaunch {
    id: TerminalId,
    launch: Launch,
    cwd: Option<PathBuf>,
    prefill: Vec<String>,
}

impl EnvironmentShell {
    fn defer_launch(&mut self, env: &EnvironmentName, deferred: DeferredLaunch) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        if live
            .deferred_launches
            .iter()
            .any(|waiting| waiting.id == deferred.id)
        {
            return;
        }
        live.deferred_launches.push(deferred);
    }

    pub(super) fn start_deferred_launches(&mut self, env: &EnvironmentName) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let deferred = std::mem::take(&mut live.deferred_launches);
        for DeferredLaunch {
            id,
            launch,
            cwd,
            prefill,
        } in deferred
        {
            if self
                .envs
                .get(env)
                .is_some_and(|live| live.data.nodes.iter().any(|node| node.id == id))
            {
                self.start_session(env, id, launch, cwd, &prefill);
            }
        }
    }

    pub(super) fn spawn_missing_sessions(&mut self, env: &EnvironmentName) -> usize {
        if !self.host_ready(env) {
            return 0;
        }
        let missing = self.envs.get(env).map_or(0, |live| {
            live.data
                .relaunches(|id| live.sessions.contains_key(&id))
                .len()
        });
        self.runtime
            .handle
            .post(Command::Terminal(TerminalCommand::OpenMissing {
                environment: env.clone(),
            }));
        self.relaunch_cloud_sessions(env);
        missing
    }

    pub(super) fn relaunch_cloud_sessions(&mut self, env: &EnvironmentName) {
        let relaunches = self.envs.get(env).map_or_else(Vec::new, |live| {
            live.data.relaunches(|id| live.sessions.contains_key(&id))
        });
        for Relaunch {
            id,
            intent,
            cwd,
            prefill,
        } in relaunches
        {
            if self.cloud_placement_of(env, id).is_some() {
                self.start_session(env, id, intent.into(), cwd, &prefill);
            }
        }
    }

    fn launch_in_runtime(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        intent: LaunchIntent,
        cwd: Option<PathBuf>,
    ) {
        self.store();
        self.runtime
            .handle
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
    ) {
        if launch.harness().is_some() && self.awaits_runner_bus(env, id) {
            self.defer_launch(
                env,
                DeferredLaunch {
                    id,
                    launch,
                    cwd,
                    prefill: prefill.to_vec(),
                },
            );
            return;
        }
        let cloud = match self.cloud_shell(env, id, &launch) {
            CloudShell::Env => None,
            CloudShell::Waiting => {
                let prefill = prefill.to_vec();
                let deferred = DeferredLaunch {
                    id,
                    launch,
                    cwd,
                    prefill,
                };
                return self.defer_launch(env, deferred);
            }
            CloudShell::Failed(error) => return self.report_cloud_shell(error),
            CloudShell::Ready { opener, types_line } => Some((opener, types_line)),
        };
        let launch = match (&cloud, LaunchIntent::try_from(launch)) {
            (None, Ok(intent)) => return self.launch_in_runtime(env, id, intent, cwd),
            (Some(_), Ok(intent)) => intent.into(),
            (_, Err(launch)) => launch,
        };
        self.spawn_session(env, id, (launch, cwd, prefill), cloud);
    }

    fn spawn_session(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        (launch, cwd, prefill): (Launch, Option<PathBuf>, &[String]),
        cloud: Option<CloudOpener>,
    ) {
        let signs_in = matches!(launch, Launch::SignIn { .. });
        if !signs_in && cloud.is_none() && !self.host_ready(env) {
            self.push_toast(ToastKind::Error, HOST_NOT_READY_TEXT.to_owned());
            return;
        }
        let harness = launch.harness();
        let (link, wsl, site) = if cloud.is_some() {
            (krad_host::HostLink::Local, None, ShellSite::Cloud)
        } else {
            self.launch_site(env, signs_in, cwd.as_deref())
        };
        let model = launch.model();
        let attached = self.opencode_attach(&launch, harness, site);
        let ResolvedLaunch {
            program,
            args,
            agent,
            vars: extra_vars,
            shell,
        } = self.resolve_launch((env, id), launch, wsl.as_ref(), site);
        let typed_cwd = cwd.clone();
        let vars = match cloud {
            Some(_) => Vec::new(),
            None => self.launch_vars(env, id, extra_vars, attached.as_ref()),
        };
        let (place, agent) = match cloud {
            Some((opener, types_line)) => {
                (TerminalPlace::Opened(opener), agent.filter(|_| types_line))
            }
            None => (shell.place(link, wsl.is_some()), agent),
        };
        let plain_shell = agent.is_none() && harness.is_none() && shell == LaunchShell::Login;
        let (agent, after_start) = agent.map_or((None, None), |(line, after)| (Some(line), after));
        let typed = agent.map(|agent| {
            let cwd = typed_cwd.filter(|_| wsl.is_none());
            TypedLine::new(agent, site, cwd, model)
        });
        let (typed_now, typed_later) = match (typed, &attached) {
            (Some(typed), Some(attached)) => (None, Some((typed, attached.clone()))),
            (Some(typed), None) => (Some(typed.bytes(None)), None),
            (None, _) => (None, None),
        };
        let launch = TerminalLaunch {
            program,
            args,
            cwd,
            vars,
            place,
            harness,
            prefill: prefill.to_vec(),
            typed: typed_now,
            tap: self.runner_sign_in_tap(env, id),
        };
        let view = self
            .terminals()
            .open_terminal(terminal_ref(env, id), launch);
        self.keep_live_session(env, id, LiveTerminal::new(view, harness));
        if plain_shell && site != ShellSite::Cloud {
            self.activate_toolchains(env, id, site);
        }
        if let Some((typed, attached)) = typed_later {
            type_attached_line(self.terminals().clone(), (env, id), typed, attached);
        }
        if let Some(prompt) = after_start {
            self.queue_prompt(env, id, prompt);
        }
        if let Some((_, resume)) = attached {
            self.keep_opencode_resume(env, id, resume);
        }
    }

    pub(super) fn keep_live_session(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        session: LiveTerminal,
    ) {
        let harness_changed = self.envs.get(env).is_some_and(|live| {
            live.data
                .nodes
                .iter()
                .any(|node| node.id == id && node.harness != session.harness)
        });
        if harness_changed {
            self.set_node(env, id, NodeField::Harness(session.harness));
        }
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.sessions.insert(id, session);
        if live.focused.is_none() {
            live.focused = Some(id);
        }
    }

    pub(super) fn role_of(&self, env: &EnvironmentName, id: TerminalId) -> AgentRole {
        let leads = self
            .envs
            .get(env)
            .is_some_and(|live| live.data.session_led_by(id).is_some());
        if leads {
            AgentRole::Lead
        } else {
            AgentRole::Member
        }
    }

    fn keep_opencode_resume(&mut self, env: &EnvironmentName, id: TerminalId, resume: ResumeId) {
        self.set_node(env, id, NodeField::Resume(Some(resume.clone())));
        self.note_opencode_terminal(env, id, resume);
    }

    pub(super) fn set_agent_state(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        state: AgentState,
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
        self.set_node(env, id, NodeField::State(state));
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
            self.note_variant_progress(env, &event);
            self.publish_event(env, group, event);
        }
        true
    }
}
