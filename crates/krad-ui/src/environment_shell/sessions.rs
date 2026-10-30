use krad_chat::{new_opencode_session_id, prepare_opencode_session, OpencodeServer};
use krad_domain::{
    attach_line, classify_agent_state, parse_wsl_path, resume_line, AgentPreset, AgentState,
    EnvironmentName, ModelId, ResumeId, ShellQuoting, TerminalId, WslPath,
};
use krad_session::{
    clear_screen_prefix, default_shell, run_spawn_job, wsl_shell_in, LiveSession, PtyInput,
    PumpMsg, SessionState, SharedKiller, SpawnJob, VtEngine, DEFAULT_COLS, DEFAULT_ROWS,
    SHELL_QUOTING, THREAD_IO_NAME,
};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::Instant;

use super::agent_hooks::AgentRole;
use super::EnvironmentShell;

const SHELL_SUBMIT: &str = "\r";
const ENGINE_FAILED_TEXT: &str = "The terminal could not start";
const RESTORE_MARKER: &str = "── restored session ──";
const SCROLLBACK_CLEARED: &[u8] = b"\x1b[3J";

pub(super) enum Launch {
    Preset(AgentPreset),
    Agent {
        preset: AgentPreset,
        prompt: String,
        model: Option<ModelId>,
    },
    Resume {
        preset: AgentPreset,
        resume: Option<ResumeId>,
    },
    Command {
        program: String,
        args: Vec<String>,
        vars: Vec<(String, String)>,
    },
}

impl Launch {
    const fn harness(&self) -> Option<AgentPreset> {
        match self {
            Self::Preset(preset) | Self::Agent { preset, .. } | Self::Resume { preset, .. }
                if preset.is_agent() =>
            {
                Some(*preset)
            }
            Self::Preset(_) | Self::Agent { .. } | Self::Resume { .. } | Self::Command { .. } => {
                None
            }
        }
    }

    fn model(&self) -> Option<ModelId> {
        match self {
            Self::Agent { model, .. } => model.clone(),
            Self::Preset(_) | Self::Resume { .. } | Self::Command { .. } => None,
        }
    }
}

struct ResolvedLaunch {
    program: String,
    args: Vec<String>,
    agent: Option<(String, Option<String>)>,
    vars: Vec<(String, String)>,
}

impl EnvironmentShell {
    pub(super) fn spawn_missing_sessions(&mut self, env: &EnvironmentName) -> usize {
        let relaunches: Vec<(TerminalId, Launch, Option<PathBuf>, Vec<String>)> =
            match self.envs.get(env) {
                Some(live) => live
                    .data
                    .nodes
                    .iter()
                    .filter(|node| node.kind.is_terminal() && !live.sessions.contains_key(&node.id))
                    .map(|node| {
                        let meta = live.data.meta.get(&node.id);
                        let group_root = node
                            .group
                            .and_then(|group| live.data.group(group))
                            .map(|group| group.root.clone());
                        let cwd = meta
                            .filter(|meta| !meta.cwd.is_empty())
                            .map(|meta| PathBuf::from(&meta.cwd))
                            .or(group_root);
                        let harness = node.harness.filter(|preset| preset.is_agent());
                        let prefill =
                            meta.filter(|_| harness.is_none())
                                .map_or(Vec::new(), |meta| {
                                    let mut lines = meta.scrollback.clone();
                                    lines.push(RESTORE_MARKER.to_owned());
                                    lines
                                });
                        let launch = harness.map_or(Launch::Preset(AgentPreset::Shell), |preset| {
                            Launch::Resume {
                                preset,
                                resume: node.resume.clone(),
                            }
                        });
                        (node.id, launch, cwd, prefill)
                    })
                    .collect(),
                None => Vec::new(),
            };
        let mut spawned = 0;
        for (id, launch, cwd, prefill) in relaunches {
            self.start_session(env, id, launch, cwd, &prefill);
            spawned += 1;
        }
        spawned
    }

    pub(super) fn start_session(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        launch: Launch,
        cwd: Option<PathBuf>,
        prefill: &[String],
    ) {
        let Some(pump) = self.pump_tx.clone() else {
            return;
        };
        let mut engine = match VtEngine::new(DEFAULT_COLS.into(), DEFAULT_ROWS.into()) {
            Ok(engine) => engine,
            Err(error) => {
                self.push_toast(format!("{ENGINE_FAILED_TEXT}: {error}"));
                return;
            }
        };
        engine.push_text_lines(prefill);
        let harness = launch.harness();
        let role = self.role_of(env, id);
        let wsl = cwd.as_deref().and_then(parse_wsl_path);
        let model = launch.model();
        let attached = self.opencode_attach(&launch, harness, wsl.is_some());
        let ResolvedLaunch {
            program,
            args,
            agent,
            vars: extra_vars,
        } = self.resolve_launch(launch, wsl.as_ref(), role);
        let cwd = if wsl.is_some() { None } else { cwd };
        let mut vars = self.bus_vars(env, id);
        vars.extend(extra_vars);
        if let Some((server, _)) = &attached {
            vars.extend(server.client_env());
        }
        let (writer_tx, writer_rx) = channel();
        if let Some((agent, after_start)) = agent {
            let typed = TypedLine {
                agent,
                in_wsl: wsl.is_some(),
                cwd: cwd.clone(),
                model,
            };
            type_agent_line(&writer_tx, typed, attached.as_ref());
            if let Some(prompt) = after_start {
                self.queue_prompt(env, id, prompt);
            }
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(live) = self.envs.get_mut(env) {
            if let Some(node) = live.data.nodes.iter_mut().find(|node| node.id == id) {
                node.harness = harness;
            }
            live.sessions.insert(
                id,
                LiveSession {
                    engine,
                    writer_tx,
                    killer: None,
                    cancelled: Arc::clone(&cancelled),
                    state: SessionState::Running,
                    last_output: Instant::now(),
                    spawned_at: Instant::now(),
                    is_agent: harness.is_some(),
                    harness,
                    hooked: false,
                    activity: None,
                    cwd: None,
                },
            );
            if live.focused.is_none() {
                live.focused = Some(id);
            }
        }
        if let Some((_, resume)) = attached {
            self.keep_opencode_resume(env, id, resume);
        }
        let job = SpawnJob {
            env: env.clone(),
            id,
            program,
            args,
            cwd,
            vars,
            pump,
            cancelled,
            inbox: writer_rx,
        };
        let spawned = std::thread::Builder::new()
            .name(THREAD_IO_NAME.to_owned())
            .spawn(move || run_spawn_job(job));
        if let Err(error) = spawned {
            self.apply_spawn_failed(env, id, error.to_string());
        }
    }

    fn role_of(&self, env: &EnvironmentName, id: TerminalId) -> AgentRole {
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
        if let Some(node) = self
            .envs
            .get_mut(env)
            .and_then(|live| live.data.nodes.iter_mut().find(|node| node.id == id))
        {
            node.resume = Some(resume.clone());
        }
        self.note_opencode_terminal(env, id, resume);
    }

    fn opencode_attach(
        &self,
        launch: &Launch,
        harness: Option<AgentPreset>,
        in_wsl: bool,
    ) -> Option<(Arc<OpencodeServer>, ResumeId)> {
        if !harness.is_some_and(AgentPreset::attaches_to_server) || in_wsl {
            return None;
        }
        let server = self.opencode_server()?.clone();
        let resume = match launch {
            Launch::Resume {
                resume: Some(resume),
                ..
            } => resume.clone(),
            Launch::Preset(_)
            | Launch::Agent { .. }
            | Launch::Resume { .. }
            | Launch::Command { .. } => new_opencode_session_id(),
        };
        Some((server, resume))
    }

    fn resolve_launch(
        &self,
        launch: Launch,
        wsl: Option<&WslPath>,
        role: AgentRole,
    ) -> ResolvedLaunch {
        let shell = || match wsl {
            Some(folder) => wsl_shell_in(&folder.distro, &folder.linux),
            None => default_shell(),
        };
        let in_wsl = wsl.is_some();
        match launch {
            Launch::Preset(preset) => {
                let (program, args) = shell();
                ResolvedLaunch {
                    program,
                    args,
                    agent: self.agent_launch_line(preset, in_wsl, "", None, role),
                    vars: Vec::new(),
                }
            }
            Launch::Agent {
                preset,
                prompt,
                model,
            } => {
                let (program, args) = shell();
                ResolvedLaunch {
                    program,
                    args,
                    agent: self.agent_launch_line(preset, in_wsl, &prompt, model.as_ref(), role),
                    vars: Vec::new(),
                }
            }
            Launch::Resume { preset, resume } => {
                let (program, args) = shell();
                let quoting = if in_wsl {
                    ShellQuoting::Posix
                } else {
                    SHELL_QUOTING
                };
                let agent = self.agent_launch_line(preset, in_wsl, "", None, role).map(
                    |(line, after_start)| match &resume {
                        Some(id) => (resume_line(&line, preset, id, quoting), after_start),
                        None => (line, after_start),
                    },
                );
                ResolvedLaunch {
                    program,
                    args,
                    agent,
                    vars: Vec::new(),
                }
            }
            Launch::Command {
                program,
                args,
                vars,
            } => ResolvedLaunch {
                program,
                args,
                agent: None,
                vars,
            },
        }
    }

    pub(super) fn apply_pump(&mut self, msg: PumpMsg) -> bool {
        match msg {
            PumpMsg::Output(env, id, bytes) => self.apply_output(&env, id, &bytes),
            PumpMsg::Ready(env, id, cwd, pid, killer) => {
                self.apply_ready(&env, id, cwd, pid, killer)
            }
            PumpMsg::Exited(env, id, code) => self.apply_exit(&env, id, code),
            PumpMsg::SpawnFailed(env, id, error) => self.apply_spawn_failed(&env, id, error),
            PumpMsg::Tick => self.apply_tick(),
        }
    }

    fn apply_output(&mut self, env: &EnvironmentName, id: TerminalId, bytes: &[u8]) -> bool {
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        let Some(session) = live.sessions.get_mut(&id) else {
            return false;
        };
        session.feed(bytes);
        if session.harness.is_some_and(AgentPreset::reopens_as_chat)
            && contains(bytes, SCROLLBACK_CLEARED)
        {
            session.hooked = true;
        }
        if session.hooked {
            return true;
        }
        let bell = session.engine.take_bell();
        let state = classify_agent_state(0.0, session.state.is_running(), bell, session.is_agent);
        self.set_agent_state(env, id, state);
        true
    }

    fn apply_ready(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cwd: Option<String>,
        _pid: Option<u32>,
        killer: SharedKiller,
    ) -> bool {
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        let Some(session) = live.sessions.get_mut(&id) else {
            return false;
        };
        session.killer = Some(killer);
        session.cwd = cwd;
        true
    }

    fn apply_exit(&mut self, env: &EnvironmentName, id: TerminalId, code: Option<i32>) -> bool {
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        let Some(session) = live.sessions.get_mut(&id) else {
            return false;
        };
        session.mark_exited(code);
        if let Some(node) = live.data.nodes.iter_mut().find(|node| node.id == id) {
            node.state = AgentState::Idle;
        }
        let raw = id.as_u64();
        match code {
            Some(code) => self.push_toast(format!("terminal #{raw} exited (code {code})")),
            None => self.push_toast(format!("terminal #{raw} exited")),
        }
        self.store();
        true
    }

    fn apply_spawn_failed(&mut self, env: &EnvironmentName, id: TerminalId, error: String) -> bool {
        let raw = id.as_u64();
        if let Some(live) = self.envs.get_mut(env) {
            if let Some(session) = live.sessions.get_mut(&id) {
                session.state = SessionState::Failed(error);
            }
        }
        self.push_toast(format!("terminal #{raw} failed to spawn"));
        true
    }

    fn apply_tick(&mut self) -> bool {
        let now = Instant::now();
        let mut changed = self.expire_toasts(now);
        self.service_agent_bus(now);
        let names: Vec<EnvironmentName> = self.envs.keys().cloned().collect();
        for env in names {
            let ids: Vec<TerminalId> = match self.envs.get(&env) {
                Some(live) => live.sessions.keys().copied().collect(),
                None => Vec::new(),
            };
            for id in ids {
                if self.reclassify(&env, id, now) {
                    changed = true;
                }
            }
        }
        changed
    }

    fn reclassify(&mut self, env: &EnvironmentName, id: TerminalId, now: Instant) -> bool {
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        let Some(session) = live.sessions.get_mut(&id) else {
            return false;
        };
        if !session.state.is_running() || session.hooked {
            return false;
        }
        let idle = now.duration_since(session.last_output).as_secs_f32();
        let bell = session.engine.take_bell();
        let state = classify_agent_state(idle, true, bell, session.is_agent);
        self.set_agent_state(env, id, state)
    }

    pub(super) fn set_agent_state(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        state: AgentState,
    ) -> bool {
        let Some(live) = self.envs.get_mut(env) else {
            return false;
        };
        let Some(node) = live.data.nodes.iter_mut().find(|node| node.id == id) else {
            return false;
        };
        if node.state == state {
            return false;
        }
        let waiting = node.state == AgentState::Working && state == AgentState::WaitingForInput;
        node.state = state;
        if state == AgentState::Working {
            live.working_since.insert(id, Instant::now());
        } else {
            live.working_since.remove(&id);
        }
        if waiting {
            let raw = id.as_u64();
            let name = env.as_str().to_owned();
            self.push_toast(format!("agent #{raw} waits for input ({name})"));
            if *env != self.active {
                self.unseen.insert(env.clone());
            }
        }
        true
    }
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

struct TypedLine {
    agent: String,
    in_wsl: bool,
    cwd: Option<PathBuf>,
    model: Option<ModelId>,
}

fn type_agent_line(
    writer: &Sender<PtyInput>,
    typed: TypedLine,
    attached: Option<&(Arc<OpencodeServer>, ResumeId)>,
) {
    let clear = clear_screen_prefix(typed.in_wsl);
    let agent = match attached {
        Some((server, resume)) => attach_line(&typed.agent, &server.url(), resume),
        None => typed.agent,
    };
    let line = PtyInput::Bytes(format!("{clear}{agent}{SHELL_SUBMIT}").into_bytes());
    let Some((server, resume)) = attached else {
        drop(writer.send(line));
        return;
    };
    let server = Arc::clone(server);
    let resume = resume.clone();
    let directory = typed.cwd.unwrap_or_default();
    let writer = writer.clone();
    std::thread::spawn(move || {
        prepare_opencode_session(&server, &resume, &directory, typed.model.as_ref())
            .unwrap_or_default();
        drop(writer.send(line));
    });
}
