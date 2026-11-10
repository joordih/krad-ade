use krad_domain::{PlanUsage, UsageFailure};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::control::{initialize_line, route, usage_line, Routed};
use crate::live::keep_stderr_tail;
use crate::parse::{claude_plan_usage, usage_failure_of};
use crate::process::{harness_command, Runner};

const CLAUDE_USAGE_ARGS: &[&str] = &[
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--no-session-persistence",
    "--settings",
    NO_HOOKS_SETTINGS,
];
const NO_HOOKS_SETTINGS: &str = r#"{"disableAllHooks":true}"#;
const INIT_REQUEST_ID: &str = "krad_usage_init";
const USAGE_REQUEST_ID: &str = "krad_usage_read";
const USAGE_READ_TIMEOUT: Duration = Duration::from_secs(45);
const CLOSE_GRACE: Duration = Duration::from_secs(2);
const CLOSE_POLL: Duration = Duration::from_millis(50);

type Reply = Result<Value, String>;

fn watch_replies(stdout: impl std::io::Read, heard: &Sender<Reply>) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(Routed::Replied { request_id, reply }) = route(&line, INIT_REQUEST_ID) {
            if request_id == USAGE_REQUEST_ID {
                heard.send(reply).unwrap_or_default();
                return;
            }
        }
    }
}

fn await_usage(heard: &Receiver<Reply>) -> Option<Reply> {
    match heard.recv_timeout(USAGE_READ_TIMEOUT) {
        Ok(reply) => Some(reply),
        Err(error) => {
            tracing::debug!(%error, "Claude Code did not answer the usage request");
            None
        }
    }
}

fn close(mut child: Child) {
    let deadline = Instant::now() + CLOSE_GRACE;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => thread::sleep(CLOSE_POLL),
            Err(error) => {
                tracing::debug!(%error, "cannot wait for the usage read to end");
                break;
            }
        }
    }
    if let Err(error) = child.kill() {
        tracing::debug!(%error, "the usage read had already ended");
    }
    if let Err(error) = child.wait() {
        tracing::debug!(%error, "cannot reap the usage read");
    }
}

fn read_of(reply: Option<Reply>, stderr: &str) -> Result<PlanUsage, UsageFailure> {
    match reply {
        Some(Ok(reply)) => claude_plan_usage(&reply),
        Some(Err(error)) => Err(usage_failure_of(&error)),
        None => Err(usage_failure_of(stderr)),
    }
}

/// # Errors
///
/// Fails when Claude Code cannot be started, is signed out, has no subscription, is rate limited
/// or answers in a shape the app cannot read.
#[tracing::instrument(skip_all)]
pub fn read_claude_usage(runner: &Runner, cwd: &Path) -> Result<PlanUsage, UsageFailure> {
    let args: Vec<String> = CLAUDE_USAGE_ARGS
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
    let mut child = harness_command(runner, cwd, &args, &[])
        .and_then(|mut command| {
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        })
        .map_err(|error| {
            tracing::debug!(%error, "cannot start Claude Code to read its usage");
            UsageFailure::Unreachable
        })?;
    let stderr_tail = Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let tail = Arc::clone(&stderr_tail);
        thread::spawn(move || keep_stderr_tail(stderr, &tail));
    }
    let (sink, heard) = channel();
    if let Some(stdout) = child.stdout.take() {
        thread::spawn(move || watch_replies(stdout, &sink));
    }
    let asked = child.stdin.take().map(|mut stdin| {
        let lines = initialize_line(INIT_REQUEST_ID) + &usage_line(USAGE_REQUEST_ID);
        stdin
            .write_all(lines.as_bytes())
            .and_then(|()| stdin.flush())
    });
    let reply = match asked {
        Some(Ok(())) => await_usage(&heard),
        Some(Err(error)) => {
            tracing::debug!(%error, "cannot ask Claude Code for its usage");
            None
        }
        None => None,
    };
    close(child);
    let stderr = stderr_tail
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    read_of(reply, &stderr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_usage_reply_is_taken_from_the_stream() {
        let stream = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"krad_usage_init","response":{"account":{}}}}"#,
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"krad_usage_read","response":{"rate_limits":{"five_hour":{"utilization":3}}}}}"#,
        ]
        .join("\n");
        let (sink, heard) = channel();
        watch_replies(stream.as_bytes(), &sink);
        let reply = await_usage(&heard);
        assert_eq!(read_of(reply, "").map(|usage| usage.windows.len()), Ok(1));
    }

    #[test]
    fn signed_out_comes_only_from_what_claude_says() {
        assert_eq!(
            read_of(None, "Invalid API key · Please run /login"),
            Err(UsageFailure::SignedOut)
        );
        assert_eq!(read_of(None, ""), Err(UsageFailure::Unreachable));
        assert_eq!(
            read_of(Some(Err("Unknown subtype".to_owned())), ""),
            Err(UsageFailure::Unreachable)
        );
        assert_eq!(
            read_of(Some(Ok(json!({"rate_limits_available": false}))), ""),
            Err(UsageFailure::NoSubscription)
        );
        assert_eq!(
            read_of(Some(Ok(json!({"rate_limits": "soon"}))), ""),
            Err(UsageFailure::Unrecognised)
        );
    }
}
