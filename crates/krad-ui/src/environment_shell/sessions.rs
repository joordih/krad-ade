mod launch;

pub(super) use launch::Launch;

use krad_chat::{prepare_opencode_session, OpencodeServer};
use krad_domain::{
    attach_line, classify_agent_state, AgentPreset, AgentState, EnvironmentName, ModelId, ResumeId,
    TerminalId,
};
use krad_session::{
    run_spawn_job, FirstInput, LiveSession, PtyInput, PumpMsg, SessionState, SharedKiller,
    SpawnJob, VtEngine, DEFAULT_COLS, DEFAULT_ROWS, THREAD_IO_NAME,
};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::Instant;

use self::launch::{LaunchShell, Placed, ResolvedLaunch};
use super::agent_hooks::AgentRole;
use super::shell_site::ShellSite;
use super::EnvironmentShell;

const SHELL_SUBMIT: &str = "\r";
const ENGINE_FAILED_TEXT: &str = "The terminal could not start";
const REMOTE_LAUNCH_FAILED_TEXT: &str = "The terminal could not start on the host";
const HOST_NOT_READY_TEXT: &str = "Connect to the environment's host first";
const RESTORE_MARKER: &str = "── restored session ──";
const SCROLLBACK_CLEARED: &[u8] = b"\x1b[3J";

impl EnvironmentShell {
    pub(super) fn spawn_missing_sessions(&mut self, env: &EnvironmentName) -> usize {
        if !self.host_ready(env) {
            return 0;
        }
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

    fn start_agent_line(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        writer: &Sender<PtyInput>,
        typed: TypedLine,
        after_start: Option<String>,
        attached: Option<&(Arc<OpencodeServer>, ResumeId)>,
    ) {
        type_agent_line(writer, typed, attached);
        if let Some(prompt) = after_start {
            self.queue_prompt(env, id, prompt);
        }
    }

    fn place(
        &mut self,
        placement: Placed,
        link: &krad_host::HostLink,
        shell: LaunchShell,
        in_wsl: bool,
    ) -> Option<Placed> {
        match placement.on(link, shell, in_wsl) {
            Ok(placed) => Some(placed),
            Err(error) => {
                self.push_toast(format!("{REMOTE_LAUNCH_FAILED_TEXT}: {error}"));
                None
            }
        }
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
        let signs_in = matches!(launch, Launch::SignIn { .. });
        if !signs_in && !self.host_ready(env) {
            self.push_toast(HOST_NOT_READY_TEXT.to_owned());
            return;
        }
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
        let (link, wsl, site) = self.launch_site(env, signs_in, cwd.as_deref());
        let model = launch.model();
        let attached = self.opencode_attach(&launch, harness, site);
        let ResolvedLaunch {
            program,
            args,
            agent,
            vars: extra_vars,
            shell,
        } = self.resolve_launch(env, launch, wsl.as_ref(), site, role);
        let vars = self.launch_vars(env, id, extra_vars, attached.as_ref());
        let typed_cwd = cwd.clone();
        let placement = Placed {
            program,
            args,
            cwd,
            vars,
            opener: None,
        };
        let Some(placed) = self.place(placement, &link, shell, wsl.is_some()) else {
            return;
        };
        let (writer_tx, writer_rx) = channel();
        let first_input = FirstInput::default();
        if agent.is_none() && harness.is_none() && shell == LaunchShell::Login {
            self.activate_toolchains(env, id, writer_tx.clone(), first_input.clone(), site);
        }
        if let Some((agent, after_start)) = agent {
            let cwd = typed_cwd.filter(|_| wsl.is_none());
            let typed = TypedLine::new(agent, site, cwd, model);
            self.start_agent_line(env, id, &writer_tx, typed, after_start, attached.as_ref());
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
                    first_input,
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
            program: placed.program,
            args: placed.args,
            cwd: placed.cwd,
            vars: placed.vars,
            pump,
            cancelled,
            inbox: writer_rx,
            opener: placed.opener,
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
        let signed_in = self.sign_in_ended(env, id, code);
        self.acp_sign_in_ended(env, id);
        let Some(live) = self.envs.get_mut(env) else {
            return signed_in;
        };
        let Some(session) = live.sessions.get_mut(&id) else {
            return signed_in;
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
    site: ShellSite,
    cwd: Option<PathBuf>,
    model: Option<ModelId>,
}

impl TypedLine {
    const fn new(
        agent: String,
        site: ShellSite,
        cwd: Option<PathBuf>,
        model: Option<ModelId>,
    ) -> Self {
        Self {
            agent,
            site,
            cwd,
            model,
        }
    }
}

fn type_agent_line(
    writer: &Sender<PtyInput>,
    typed: TypedLine,
    attached: Option<&(Arc<OpencodeServer>, ResumeId)>,
) {
    let clear = typed.site.clear_prefix();
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
