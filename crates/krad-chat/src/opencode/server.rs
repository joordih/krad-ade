use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use ureq::typestate::{WithBody, WithoutBody};
use ureq::{Agent, RequestBuilder};

use super::events::ServerEvent;
use super::service;
use crate::live::keep_stderr_tail;
use crate::process::{harness_command, Runner};

const SERVE_ARGS: [&str; 4] = ["serve", "--stdio", "--port", "0"];
const PASSWORD_VAR: &str = "OPENCODE_PASSWORD";
const QUIET_VARS: [(&str, &str); 1] = [("OPENCODE_DISABLE_AUTOUPDATE", "1")];
const USER: &str = "opencode";
const BASIC: &str = "Basic ";
const AUTHORIZATION: &str = "authorization";
const EVENT_PATH: &str = "/api/event";
const DATA_PREFIX: &str = "data:";
const LOCATION_QUERY: &str = "location[directory]";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
const PASSWORD_WORDS: usize = 4;
const MAX_NESTING: usize = 16;
const NOT_FOUND: u16 = 404;

#[derive(Debug, Error)]
pub enum OpencodeError {
    #[error("cannot run opencode: {0}")]
    Io(#[from] std::io::Error),
    #[error("opencode did not start: {0}")]
    NotStarted(String),
    #[error("opencode refused the request: {0}")]
    Http(#[from] ureq::Error),
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

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

pub(crate) struct Delivery {
    pub(crate) event: ServerEvent,
    pub(crate) nested: bool,
}

pub(crate) type Sink = Box<dyn FnMut(Delivery) + Send>;

#[derive(Default)]
struct Routes {
    sinks: HashMap<String, (u64, Sink)>,
    parents: HashMap<String, String>,
    next: u64,
}

impl Routes {
    fn root(&self, session: &str) -> String {
        let mut at = session;
        for _ in 0..MAX_NESTING {
            match self.parents.get(at) {
                Some(parent) => at = parent,
                None => break,
            }
        }
        at.to_owned()
    }

    fn deliver(&mut self, event: ServerEvent) {
        if let Some((child, parent)) = event.parent() {
            self.parents.insert(child.to_owned(), parent.to_owned());
        }
        let session = event.session();
        let root = self.root(session);
        let nested = root != session;
        if let Some((_, sink)) = self.sinks.get_mut(&root) {
            sink(Delivery { event, nested });
        }
    }
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
        if !runner.in_wsl() {
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
        env.push((PASSWORD_VAR.to_owned(), password.0.clone()));
        let args: Vec<String> = SERVE_ARGS.iter().map(|arg| (*arg).to_owned()).collect();
        let mut child = harness_command(runner, cwd, &args, &env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let lease = child.stdin.take();
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
        let endpoint = Endpoint::new(ready.url, Some(password));
        Ok(Self::serve(endpoint, Host::Private { child, lease }, stay))
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

    fn url_of(&self, path: &str) -> String {
        format!("{}{path}", self.url())
    }

    fn located<B>(request: RequestBuilder<B>, directory: Option<&str>) -> RequestBuilder<B> {
        match directory {
            Some(directory) => request.query(LOCATION_QUERY, directory),
            None => request,
        }
    }

    fn without_body(&self, request: RequestBuilder<WithoutBody>) -> RequestBuilder<WithoutBody> {
        self.endpoint().sign(request)
    }

    fn with_body(&self, request: RequestBuilder<WithBody>) -> RequestBuilder<WithBody> {
        self.endpoint().sign(request)
    }

    pub(crate) fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        directory: Option<&str>,
    ) -> Result<T, OpencodeError> {
        let request = Self::located(
            self.without_body(self.agent.get(&self.url_of(path))),
            directory,
        );
        let envelope: Envelope<T> = request.call()?.body_mut().read_json()?;
        Ok(envelope.data)
    }

    pub(crate) fn read<T: DeserializeOwned>(&self, path: &str) -> Result<T, OpencodeError> {
        let request = self.without_body(self.agent.get(&self.url_of(path)));
        Ok(request.call()?.body_mut().read_json()?)
    }

    pub(crate) fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, OpencodeError> {
        let request = self.with_body(self.agent.post(&self.url_of(path)));
        let envelope: Envelope<T> = request.send_json(body)?.body_mut().read_json()?;
        Ok(envelope.data)
    }

    pub(crate) fn send<B: Serialize>(&self, path: &str, body: &B) -> Result<(), OpencodeError> {
        self.with_body(self.agent.post(&self.url_of(path)))
            .send_json(body)?;
        Ok(())
    }

    pub(crate) fn nudge(&self, path: &str) -> Result<(), OpencodeError> {
        self.with_body(self.agent.post(&self.url_of(path)))
            .send_empty()?;
        Ok(())
    }

    pub(crate) fn delete(&self, path: &str) -> Result<(), OpencodeError> {
        self.without_body(self.agent.delete(&self.url_of(path)))
            .call()?;
        Ok(())
    }

    pub(crate) fn exists(&self, path: &str) -> Result<bool, OpencodeError> {
        match self.without_body(self.agent.get(&self.url_of(path))).call() {
            Ok(_) => Ok(true),
            Err(ureq::Error::StatusCode(NOT_FOUND)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn patch<B: Serialize>(&self, path: &str, body: &B) -> Result<(), OpencodeError> {
        self.with_body(self.agent.patch(&self.url_of(path)))
            .send_json(body)?;
        Ok(())
    }

    pub(crate) fn put<B: Serialize>(&self, path: &str, body: &B) -> Result<(), OpencodeError> {
        self.with_body(self.agent.put(&self.url_of(path)))
            .send_json(body)?;
        Ok(())
    }
}

impl Drop for OpencodeServer {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        let host = self.host.get_mut().unwrap_or_else(PoisonError::into_inner);
        let Host::Private { child, lease } = host else {
            return;
        };
        lease.take();
        child.kill().unwrap_or_default();
        drop(child.wait());
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

fn pump(reader: impl BufRead, routes: &Mutex<Routes>) {
    for line in reader.lines().map_while(Result::ok) {
        let Some(data) = line.strip_prefix(DATA_PREFIX) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<ServerEvent>(data.trim()) else {
            continue;
        };
        routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .deliver(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn events_of_a_subagent_reach_the_chat_that_started_it() {
        let routes = Mutex::new(Routes::default());
        let (sender, receiver) = channel();
        routes
            .lock()
            .unwrap_or_else(|_| unreachable!("the fixture is valid"))
            .sinks
            .insert(
                "ses_root".to_owned(),
                (
                    1,
                    Box::new(move |delivery: Delivery| {
                        sender
                            .send(delivery.nested)
                            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
                    }),
                ),
            );
        let feed = concat!(
            "data: {\"type\":\"session.created\",\"data\":{\"sessionID\":\"ses_child\",\"parentID\":\"ses_root\"}}\n\n",
            ": heartbeat\n\n",
            "data: {\"type\":\"session.execution.succeeded\",\"data\":{\"sessionID\":\"ses_child\"}}\n\n",
            "data: {\"type\":\"session.execution.succeeded\",\"data\":{\"sessionID\":\"ses_root\"}}\n\n",
            "data: {\"type\":\"session.execution.succeeded\",\"data\":{\"sessionID\":\"ses_other\"}}\n\n",
            "data: {\"type\":\"websearch.updated\",\"data\":{\"sessionID\":\"ses_root\"}}\n\n",
        );
        pump(feed.as_bytes(), &routes);
        let nested: Vec<bool> = receiver.try_iter().collect();
        assert_eq!(nested, [true, true, false]);
    }

    #[test]
    fn the_password_never_shows_in_debug_output() {
        let password = new_password();
        assert_eq!(format!("{password:?}"), "[REDACTED]");
        assert!(password.0.len() > PASSWORD_WORDS * 8);
    }
}
