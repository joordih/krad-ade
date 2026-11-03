use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::Deserialize;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use ureq::{Agent, RequestBuilder};

use super::service;

mod requests;
mod routes;

use crate::live::keep_stderr_tail;
use crate::process::{harness_command, ssh_command_reading_secret, Runner};
use routes::{pump, Routes};
pub(crate) use routes::{Delivery, Sink};

const SERVE_ARGS: [&str; 4] = ["serve", "--stdio", "--port", "0"];
const PASSWORD_VAR: &str = "OPENCODE_PASSWORD";
const QUIET_VARS: [(&str, &str); 1] = [("OPENCODE_DISABLE_AUTOUPDATE", "1")];
const USER: &str = "opencode";
const BASIC: &str = "Basic ";
const AUTHORIZATION: &str = "authorization";
const EVENT_PATH: &str = "/api/event";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
const PASSWORD_WORDS: usize = 4;
const LOCAL_URL_PREFIX: &str = "http://127.0.0.1:";
const PORT_SEPARATOR: char = ':';
const PATH_SEPARATOR: char = '/';
const LINE_END: &[u8] = b"\n";

#[derive(Debug, Error)]
pub enum OpencodeError {
    #[error("cannot run opencode: {0}")]
    Io(#[from] std::io::Error),
    #[error("opencode did not start: {0}")]
    NotStarted(String),
    #[error("opencode refused the request: {0}")]
    Http(#[from] ureq::Error),
    #[error("cannot reach opencode on the host: {0}")]
    Host(#[from] krad_host::HostError),
}

fn port_of(url: &str) -> Option<u16> {
    url.trim_end_matches(PATH_SEPARATOR)
        .rsplit(PORT_SEPARATOR)
        .next()?
        .parse()
        .ok()
}

fn local_url(port: u16) -> String {
    format!("{LOCAL_URL_PREFIX}{port}")
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
pub(super) struct Secret(String);

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

fn new_password() -> Secret {
    let bytes: Vec<u8> = (0..PASSWORD_WORDS)
        .flat_map(|_| RandomState::new().build_hasher().finish().to_le_bytes())
        .collect();
    Secret(URL_SAFE_NO_PAD.encode(bytes))
}

#[derive(Deserialize)]
struct Ready {
    url: String,
}

#[derive(Clone)]
pub(super) struct Endpoint {
    pub(super) url: String,
    password: Option<Secret>,
}

impl Endpoint {
    pub(super) const fn new(url: String, password: Option<Secret>) -> Self {
        Self { url, password }
    }

    pub(super) fn sign<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        let Some(password) = &self.password else {
            return request;
        };
        let credentials = STANDARD.encode(format!("{USER}:{}", password.0));
        request.header(AUTHORIZATION, format!("{BASIC}{credentials}"))
    }
}

enum Host {
    Private {
        child: Child,
        lease: Option<ChildStdin>,
        forward: Option<krad_host::Forward>,
    },
    Shared,
}

type Relocate = fn() -> Option<Endpoint>;

const fn stay() -> Option<Endpoint> {
    None
}

pub struct OpencodeServer {
    endpoint: Arc<RwLock<Endpoint>>,
    agent: Agent,
    routes: Arc<Mutex<Routes>>,
    closed: Arc<AtomicBool>,
    host: Mutex<Host>,
}

impl fmt::Debug for OpencodeServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpencodeServer")
            .field("url", &self.url())
            .finish_non_exhaustive()
    }
}

impl OpencodeServer {
    /// # Errors
    ///
    /// Fails when opencode cannot be started or never reports where it listens.
    pub fn start(
        runner: &Runner,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Result<Arc<Self>, OpencodeError> {
        let mut env = env.to_vec();
        env.extend(
            QUIET_VARS
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
        );
        if !runner.in_wsl() && !runner.on_ssh() {
            if let Ok(endpoint) = service::attach(runner, cwd, &env) {
                return Ok(Self::serve(endpoint, Host::Shared, service::discover));
            }
        }
        Self::start_private(runner, cwd, env)
    }

    fn start_private(
        runner: &Runner,
        cwd: &Path,
        mut env: Vec<(String, String)>,
    ) -> Result<Arc<Self>, OpencodeError> {
        let password = new_password();
        let args: Vec<String> = SERVE_ARGS.iter().map(|arg| (*arg).to_owned()).collect();
        let mut command = match runner {
            Runner::Ssh { link, program } => {
                ssh_command_reading_secret(link, program, cwd, &args, &env, PASSWORD_VAR)?
            }
            Runner::Native(_) | Runner::Wsl { .. } => {
                env.push((PASSWORD_VAR.to_owned(), password.0.clone()));
                harness_command(runner, cwd, &args, &env)?
            }
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut lease = child.stdin.take();
        if runner.on_ssh() {
            if let Some(stdin) = lease.as_mut() {
                stdin.write_all(password.0.as_bytes())?;
                stdin.write_all(LINE_END)?;
                stdin.flush()?;
            }
        }
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let tail = Arc::clone(&stderr_tail);
            thread::spawn(move || keep_stderr_tail(stderr, &tail));
        }
        let ready = child.stdout.take().and_then(|stdout| {
            let mut lines = BufReader::new(stdout).lines();
            let ready = lines
                .by_ref()
                .map_while(Result::ok)
                .find_map(|line| serde_json::from_str::<Ready>(line.trim()).ok());
            thread::spawn(move || lines.map_while(Result::ok).for_each(drop));
            ready
        });
        let Some(ready) = ready else {
            child.kill().unwrap_or_default();
            drop(child.wait());
            let tail = stderr_tail
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .trim()
                .to_owned();
            return Err(OpencodeError::NotStarted(tail));
        };
        let (url, forward) = match runner {
            Runner::Ssh {
                link: krad_host::HostLink::Ssh(ssh),
                ..
            } => {
                let remote = port_of(&ready.url)
                    .ok_or_else(|| OpencodeError::NotStarted(ready.url.clone()))?;
                let forward = ssh.forward_local(remote)?;
                (local_url(forward.local_port), Some(forward))
            }
            _ => (ready.url, None),
        };
        let endpoint = Endpoint::new(url, Some(password));
        let host = Host::Private {
            child,
            lease,
            forward,
        };
        Ok(Self::serve(endpoint, host, stay))
    }

    fn serve(endpoint: Endpoint, host: Host, relocate: Relocate) -> Arc<Self> {
        let server = Arc::new(Self {
            agent: Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .build()
                .new_agent(),
            routes: Arc::new(Mutex::new(Routes::default())),
            closed: Arc::new(AtomicBool::new(false)),
            endpoint: Arc::new(RwLock::new(endpoint)),
            host: Mutex::new(host),
        });
        let feed = Arc::clone(&server.endpoint);
        let routes = Arc::clone(&server.routes);
        let closed = Arc::clone(&server.closed);
        thread::spawn(move || listen(&feed, &routes, &closed, relocate));
        server
    }

    fn endpoint(&self) -> Endpoint {
        self.endpoint
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[must_use]
    pub fn url(&self) -> String {
        self.endpoint().url
    }

    #[must_use]
    pub fn client_env(&self) -> Vec<(String, String)> {
        self.endpoint()
            .password
            .map(|password| (PASSWORD_VAR.to_owned(), password.0))
            .into_iter()
            .collect()
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        match &mut *self.host.lock().unwrap_or_else(PoisonError::into_inner) {
            Host::Private { child, .. } => child.try_wait().is_ok_and(|status| status.is_none()),
            Host::Shared => true,
        }
    }

    pub(crate) fn subscribe(&self, session: &str, sink: Sink) -> u64 {
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        routes.next += 1;
        let token = routes.next;
        routes.sinks.insert(session.to_owned(), (token, sink));
        token
    }

    pub fn watch_catalog(&self, sink: impl FnMut() + Send + 'static) {
        self.routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .catalog = Some(Box::new(sink));
    }

    pub(crate) fn unsubscribe(&self, session: &str, token: u64) {
        let mut routes = self.routes.lock().unwrap_or_else(PoisonError::into_inner);
        if routes
            .sinks
            .get(session)
            .is_some_and(|(held, _)| *held == token)
        {
            routes.sinks.remove(session);
        }
    }
}

impl Drop for OpencodeServer {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        let host = self.host.get_mut().unwrap_or_else(PoisonError::into_inner);
        let Host::Private {
            child,
            lease,
            forward,
        } = host
        else {
            return;
        };
        lease.take();
        child.kill().unwrap_or_default();
        drop(child.wait());
        forward.take();
    }
}

fn listen(
    endpoint: &RwLock<Endpoint>,
    routes: &Mutex<Routes>,
    closed: &AtomicBool,
    relocate: Relocate,
) {
    let agent = Agent::new_with_defaults();
    while !closed.load(Ordering::SeqCst) {
        let current = endpoint
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let url = format!("{}{EVENT_PATH}", current.url);
        match current.sign(agent.get(&url)).call() {
            Ok(response) => pump(BufReader::new(response.into_body().into_reader()), routes),
            Err(_) => {
                if let Some(moved) = relocate() {
                    *endpoint.write().unwrap_or_else(PoisonError::into_inner) = moved;
                }
            }
        }
        thread::sleep(RECONNECT_DELAY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_opencode_listens_on_comes_from_its_url() {
        assert_eq!(port_of("http://127.0.0.1:41234"), Some(41_234));
        assert_eq!(port_of("http://127.0.0.1:41234/"), Some(41_234));
        assert_eq!(port_of("http://localhost"), None);
        assert_eq!(local_url(5000), "http://127.0.0.1:5000");
    }
    #[test]
    fn the_password_never_shows_in_debug_output() {
        let password = new_password();
        assert_eq!(format!("{password:?}"), "[REDACTED]");
        assert!(password.0.len() > PASSWORD_WORDS * 8);
    }
}
