mod output;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

use krad_host::{HostCommand, HostLink};
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;
use tracing::debug;

use super::absolute;
use crate::acp::ids::TerminalId;
use crate::acp::paths::AgentPaths;
use crate::acp::rpc::{EmptyResponse, ErrorCode, RpcError};
use crate::acp::terminals::{
    CreateTerminalRequest, CreateTerminalResponse, TerminalExitStatus, TerminalOutputResponse,
};
use crate::process::wsl_command;
use output::Output;

const ID_PREFIX: &str = "terminal-";
const DEFAULT_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const CHUNK_BYTES: usize = 8192;
const STREAMS: usize = 2;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const NOT_STARTED_TEXT: &str = "The command could not start";
const UNKNOWN_TEXT: &str = "No terminal has this id";

#[derive(Debug)]
struct State {
    output: Output,
    open_streams: usize,
    exit: Option<TerminalExitStatus>,
    released: bool,
}

#[derive(Debug)]
struct Terminal {
    child: Mutex<Child>,
    state: Mutex<State>,
    changed: Condvar,
}

impl Terminal {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn snapshot(&self) -> TerminalOutputResponse {
        let state = self.state();
        TerminalOutputResponse {
            output: state.output.text(),
            truncated: state.output.truncated(),
            exit_status: state.exit.clone(),
        }
    }

    fn kill(&self) {
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        platform::kill(&mut child);
    }

    fn wait_for_exit(&self) -> TerminalExitStatus {
        let state = self
            .changed
            .wait_while(self.state(), |state| state.exit.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        state.exit.clone().unwrap_or_default()
    }

    fn pump(&self, mut stream: impl Read) {
        let mut buffer = [0; CHUNK_BYTES];
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            self.state()
                .output
                .push(buffer.get(..read).unwrap_or_default());
        }
        let mut state = self.state();
        state.open_streams = state.open_streams.saturating_sub(1);
        drop(state);
        self.changed.notify_all();
    }

    fn watch(&self) {
        let exit = loop {
            let polled = self
                .child
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .try_wait();
            match polled {
                Ok(Some(status)) => break platform::exit_status(status),
                Ok(None) => thread::sleep(POLL_INTERVAL),
                Err(error) => {
                    debug!(%error, "lost track of an ACP terminal process");
                    break TerminalExitStatus::default();
                }
            }
        };
        let (mut state, _) = self
            .changed
            .wait_timeout_while(self.state(), DRAIN_GRACE, |state| state.open_streams > 0)
            .unwrap_or_else(PoisonError::into_inner);
        state.exit = Some(exit);
        drop(state);
        self.changed.notify_all();
    }
}

#[derive(Debug)]
struct Registry {
    host: HostLink,
    paths: AgentPaths,
    next: AtomicU64,
    terminals: Mutex<BTreeMap<TerminalId, Arc<Terminal>>>,
}

impl Registry {
    fn terminals(&self) -> MutexGuard<'_, BTreeMap<TerminalId, Arc<Terminal>>> {
        self.terminals
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        for terminal in self.terminals().values() {
            terminal.kill();
        }
    }
}

#[derive(Clone, Debug)]
pub struct HostTerminals {
    registry: Arc<Registry>,
}

impl HostTerminals {
    #[must_use]
    pub fn new(host: HostLink, paths: AgentPaths) -> Self {
        Self {
            registry: Arc::new(Registry {
                host,
                paths,
                next: AtomicU64::new(0),
                terminals: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    #[must_use]
    pub fn snapshot(&self, id: &TerminalId) -> Option<TerminalOutputResponse> {
        self.registry
            .terminals()
            .get(id)
            .map(|terminal| terminal.snapshot())
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn create(
        &self,
        request: &CreateTerminalRequest,
    ) -> Result<CreateTerminalResponse, RpcError> {
        let (mut built, stdin) = self.command(request)?;
        platform::own_group(&mut built);
        built
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = built.spawn().map_err(not_started)?;
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
        let limit = request
            .output_byte_limit
            .and_then(|limit| usize::try_from(limit).ok())
            .unwrap_or(DEFAULT_OUTPUT_LIMIT);
        let terminal = Arc::new(Terminal {
            child: Mutex::new(child),
            state: Mutex::new(State {
                output: Output::new(limit),
                open_streams: STREAMS,
                exit: None,
                released: false,
            }),
            changed: Condvar::new(),
        });
        listen(&terminal, stdout);
        listen(&terminal, stderr);
        let watched = Arc::clone(&terminal);
        thread::spawn(move || watched.watch());
        let number = self.registry.next.fetch_add(1, Ordering::Relaxed) + 1;
        let terminal_id = TerminalId(format!("{ID_PREFIX}{number}"));
        self.registry
            .terminals()
            .insert(terminal_id.clone(), terminal);
        debug!(terminal = %terminal_id.0, "started an ACP terminal");
        Ok(CreateTerminalResponse { terminal_id })
    }

    pub(super) fn command(
        &self,
        request: &CreateTerminalRequest,
    ) -> Result<(Command, Stdio), RpcError> {
        let Registry { host, paths, .. } = &*self.registry;
        let cwd = request.cwd.as_deref().unwrap_or(paths.agent_cwd());
        absolute(paths, host, cwd)?;
        if let Some(distro) = paths.wsl_distro() {
            let env: Vec<(String, String)> = request
                .env
                .iter()
                .map(|variable| (variable.name.clone(), variable.value.clone()))
                .collect();
            let command = wsl_command(
                distro,
                &cwd.to_string_lossy(),
                &request.command,
                &request.args,
                &env,
            );
            return Ok((command, Stdio::null()));
        }
        let mut command = HostCommand::new(&request.command);
        command
            .args(&request.args)
            .current_dir(cwd)
            .in_login_shell();
        for variable in &request.env {
            command.env(&variable.name, &variable.value);
        }
        let stdin = if host.is_local() {
            Stdio::null()
        } else {
            command.end_with_stdin();
            Stdio::piped()
        };
        Ok((command.build(host).map_err(not_started)?, stdin))
    }

    pub(super) fn output(&self, id: &TerminalId) -> Result<TerminalOutputResponse, RpcError> {
        Ok(self.live(id)?.snapshot())
    }

    pub(super) fn wait_for_exit(&self, id: &TerminalId) -> Result<TerminalExitStatus, RpcError> {
        Ok(self.live(id)?.wait_for_exit())
    }

    pub(super) fn kill(&self, id: &TerminalId) -> Result<EmptyResponse, RpcError> {
        self.live(id)?.kill();
        Ok(EmptyResponse {})
    }

    pub(super) fn release(&self, id: &TerminalId) -> Result<EmptyResponse, RpcError> {
        let terminal = self.live(id)?;
        terminal.state().released = true;
        terminal.kill();
        debug!(terminal = %id.0, "released an ACP terminal");
        Ok(EmptyResponse {})
    }

    fn live(&self, id: &TerminalId) -> Result<Arc<Terminal>, RpcError> {
        self.registry
            .terminals()
            .get(id)
            .filter(|terminal| !terminal.state().released)
            .cloned()
            .ok_or_else(|| RpcError::new(ErrorCode::INVALID_PARAMS, UNKNOWN_TEXT))
    }
}

fn listen(terminal: &Arc<Terminal>, stream: Option<impl Read + Send + 'static>) {
    let Some(stream) = stream else {
        let mut state = terminal.state();
        state.open_streams = state.open_streams.saturating_sub(1);
        return;
    };
    let pumped = Arc::clone(terminal);
    thread::spawn(move || pumped.pump(stream));
}

fn not_started(error: impl std::fmt::Display) -> RpcError {
    debug!("an ACP terminal could not start");
    RpcError::new(
        ErrorCode::INTERNAL_ERROR,
        format!("{NOT_STARTED_TEXT}: {error}"),
    )
}
