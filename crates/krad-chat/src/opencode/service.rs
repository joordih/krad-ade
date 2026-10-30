use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};
use ureq::Agent;

use super::server::{Endpoint, OpencodeError, Secret};
use crate::process::{harness_command, Runner};

const START_ARGS: [&str; 2] = ["service", "start"];
const STATE_HOME_VAR: &str = "XDG_STATE_HOME";
const DEFAULT_STATE_HOME: [&str; 2] = [".local", "state"];
const STATE_FOLDER: &str = "opencode";
const REGISTRATION_FILE: &str = "service.json";
const INFO_PATH: &str = "/api/info";
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const READY_POLL: Duration = Duration::from_millis(100);
const START_FAILED: &str = "the opencode background service did not start:";
const NEVER_READY: &str = "the opencode background service never became ready";

#[derive(Debug, Deserialize)]
struct Registration {
    url: String,
    pid: u32,
    version: Option<String>,
    password: Option<Secret>,
}

#[derive(Debug, Deserialize)]
struct Health {
    version: String,
    pid: u32,
}

impl Registration {
    fn answered_by(&self, health: &Health) -> bool {
        self.pid == health.pid
            && self
                .version
                .as_ref()
                .is_none_or(|version| *version == health.version)
    }
}

impl From<Registration> for Endpoint {
    fn from(registration: Registration) -> Self {
        Self::new(registration.url, registration.password)
    }
}

fn state_home() -> Option<PathBuf> {
    std::env::var_os(STATE_HOME_VAR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::home_dir().map(|home| {
                DEFAULT_STATE_HOME
                    .iter()
                    .fold(home, |path, part| path.join(part))
            })
        })
}

fn registration_file() -> Option<PathBuf> {
    Some(state_home()?.join(STATE_FOLDER).join(REGISTRATION_FILE))
}

fn health_of(endpoint: &Endpoint) -> Option<Health> {
    let agent = Agent::config_builder()
        .timeout_global(Some(HEALTH_TIMEOUT))
        .build()
        .new_agent();
    let url = format!("{}{INFO_PATH}", endpoint.url);
    endpoint
        .sign(agent.get(&url))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()
}

pub(super) fn discover() -> Option<Endpoint> {
    let text = std::fs::read_to_string(registration_file()?).ok()?;
    let registration: Registration = serde_json::from_str(&text).ok()?;
    let endpoint = Endpoint::new(registration.url.clone(), registration.password.clone());
    let health = health_of(&endpoint)?;
    registration
        .answered_by(&health)
        .then(|| registration.into())
}

fn wait_until_ready() -> Result<Endpoint, OpencodeError> {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Some(endpoint) = discover() {
            return Ok(endpoint);
        }
        if Instant::now() >= deadline {
            return Err(OpencodeError::NotStarted(NEVER_READY.to_owned()));
        }
        thread::sleep(READY_POLL);
    }
}

pub(super) fn attach(
    runner: &Runner,
    cwd: &Path,
    env: &[(String, String)],
) -> Result<Endpoint, OpencodeError> {
    if let Some(endpoint) = discover() {
        return Ok(endpoint);
    }
    let args: Vec<String> = START_ARGS.iter().map(|arg| (*arg).to_owned()).collect();
    let status = harness_command(runner, cwd, &args, env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(OpencodeError::NotStarted(format!(
            "{START_FAILED} {status}"
        )));
    }
    wait_until_ready()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(line: &str) -> Registration {
        serde_json::from_str(line).unwrap_or_else(|_| unreachable!("the fixture is valid"))
    }

    fn health(line: &str) -> Health {
        serde_json::from_str(line).unwrap_or_else(|_| unreachable!("the fixture is valid"))
    }

    #[test]
    fn a_registration_counts_only_when_its_own_process_answers() {
        let listed = registration(
            r#"{"id":"a","version":"2.2.2","url":"http://127.0.0.1:49374","pid":42,"password":"p"}"#,
        );
        assert!(listed.answered_by(&health(r#"{"version":"2.2.2","pid":42}"#)));
        assert!(!listed.answered_by(&health(r#"{"version":"2.2.2","pid":7}"#)));
        assert!(!listed.answered_by(&health(r#"{"version":"2.3.0","pid":42}"#)));
        let unversioned = registration(r#"{"url":"http://127.0.0.1:49374","pid":42}"#);
        assert!(unversioned.answered_by(&health(r#"{"version":"2.3.0","pid":42}"#)));
        assert!(unversioned.password.is_none());
    }

    #[test]
    fn a_registered_password_never_shows_in_debug_output() {
        let listed = registration(r#"{"url":"http://127.0.0.1:1","pid":1,"password":"hunter2"}"#);
        assert!(!format!("{listed:?}").contains("hunter2"));
    }
}
