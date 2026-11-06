use krad_host::{HostCommand, HostLink};
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::protocol::{answer_line, notification_line, request_line, Incoming, Received};
use super::McpError;

const INBOX_BOUND: usize = 64;
const STDERR_TAIL_LINES: usize = 20;
const SHUTDOWN_GRACE: Duration = Duration::from_millis(300);
const SHUTDOWN_POLL: Duration = Duration::from_millis(20);
const STDERR_GRACE: Duration = Duration::from_millis(200);
const LINE_END: &[u8] = b"\n";
const TAIL_JOINER: &str = "\n";
const PATH_VAR: &str = "PATH";

enum Piece {
    Message(Incoming),
    Closed,
}

pub(super) struct StdioLink {
    child: Child,
    stdin: Option<ChildStdin>,
    inbox: Receiver<Piece>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    stderr_reader: Option<JoinHandle<()>>,
    closed: bool,
}

pub(super) struct StdioLaunch<'a> {
    pub(super) command: &'a str,
    pub(super) args: &'a [String],
    pub(super) env: &'a [(String, String)],
    pub(super) cwd: &'a Path,
    pub(super) host: &'a HostLink,
}

fn command(launch: &StdioLaunch<'_>) -> Result<(Command, Option<PathBuf>), McpError> {
    if launch.host.is_local() {
        let mut command = Command::new(launch.command);
        command
            .args(launch.args)
            .envs(launch.env.iter().map(|(name, value)| (name, value)))
            .current_dir(launch.cwd);
        return Ok((command, None));
    }
    let mut remote = HostCommand::new(launch.command);
    remote
        .args(launch.args)
        .current_dir(launch.cwd)
        .in_login_shell();
    let (path, env) = split_path(launch.env);
    if let Some(dir) = path {
        remote.prepend_path(dir);
    }
    let mut env_file = None;
    if let (HostLink::Remote(ssh), false) = (launch.host, env.is_empty()) {
        let file = ssh
            .write_env_file(&env)
            .map_err(|error| McpError::EnvOnHost(error.to_string()))?;
        remote.source_env_file(&file);
        env_file = Some(file);
    }
    let built = match remote.build(launch.host) {
        Ok(built) => built,
        Err(error) => {
            if let Some(file) = &env_file {
                launch.host.remove_file(file).ok();
            }
            return Err(McpError::Spawn(error.into()));
        }
    };
    Ok((built, env_file))
}

fn split_path(env: &[(String, String)]) -> (Option<&str>, Vec<(String, String)>) {
    let path = env
        .iter()
        .find(|(name, _)| name == PATH_VAR)
        .map(|(_, value)| value.as_str());
    let rest = env
        .iter()
        .filter(|(name, _)| name != PATH_VAR)
        .cloned()
        .collect();
    (path, rest)
}

fn pump(stdout: impl Read, inbox: &SyncSender<Piece>) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Incoming>(line.trim()) else {
            continue;
        };
        if inbox.send(Piece::Message(message)).is_err() {
            return;
        }
    }
    inbox.send(Piece::Closed).unwrap_or_default();
}

fn keep_tail(stderr: impl Read, tail: &Mutex<VecDeque<String>>) {
    for line in BufReader::new(stderr).lines() {
        let Ok(line) = line else { break };
        let mut kept = tail.lock().unwrap_or_else(PoisonError::into_inner);
        if kept.len() == STDERR_TAIL_LINES {
            kept.pop_front();
        }
        kept.push_back(line);
    }
}

impl StdioLink {
    pub(super) fn spawn(launch: &StdioLaunch<'_>) -> Result<Self, McpError> {
        let (mut command, env_file) = command(launch)?;
        let spawned = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                if let Some(file) = env_file {
                    launch.host.remove_file(&file).ok();
                }
                return Err(McpError::Spawn(error));
            }
        };
        let (sender, inbox) = sync_channel(INBOX_BOUND);
        if let Some(stdout) = child.stdout.take() {
            drop(thread::spawn(move || pump(stdout, &sender)));
        }
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let stderr_reader = child.stderr.take().map(|pipe| {
            let tail = Arc::clone(&stderr);
            thread::spawn(move || keep_tail(pipe, &tail))
        });
        let stdin = child.stdin.take();
        Ok(Self {
            child,
            stdin,
            inbox,
            stderr,
            stderr_reader,
            closed: false,
        })
    }

    fn exited(&self) -> McpError {
        if let Some(reader) = &self.stderr_reader {
            let deadline = Instant::now() + STDERR_GRACE;
            while !reader.is_finished() && Instant::now() < deadline {
                thread::sleep(SHUTDOWN_POLL);
            }
        }
        let tail = self.stderr.lock().unwrap_or_else(PoisonError::into_inner);
        McpError::Exited {
            stderr: tail
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(TAIL_JOINER),
        }
    }

    fn write_line(&mut self, line: &str) -> Result<(), McpError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(self.exited());
        };
        let written = stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.write_all(LINE_END))
            .and_then(|()| stdin.flush());
        written.map_err(|_| self.exited())
    }

    pub(super) fn notify(&mut self, method: &str) -> Result<(), McpError> {
        self.write_line(&notification_line(method))
    }

    pub(super) fn request(
        &mut self,
        id: u64,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, McpError> {
        if self.closed {
            return Err(self.exited());
        }
        self.write_line(&request_line(id, method, params))?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.inbox.recv_timeout(left) {
                Ok(Piece::Message(message)) => match message.sort(id) {
                    Received::Reply(reply) => return reply,
                    Received::Request { id: asked, method } => {
                        self.write_line(&answer_line(&asked, &method))?;
                    }
                    Received::Other => {}
                },
                Ok(Piece::Closed) | Err(RecvTimeoutError::Disconnected) => {
                    self.closed = true;
                    return Err(self.exited());
                }
                Err(RecvTimeoutError::Timeout) => return Err(McpError::TimedOut(timeout)),
            }
        }
    }
}

impl Drop for StdioLink {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            thread::sleep(SHUTDOWN_POLL);
        }
        self.child.kill().unwrap_or_default();
        self.child.wait().map_or_default(drop);
    }
}

#[cfg(test)]
mod tests {
    use super::split_path;

    #[test]
    fn path_is_taken_out_of_the_env_file_so_the_login_env_cannot_replace_it() {
        let env = vec![
            ("KRAD_ADE_ENDPOINT".to_owned(), "127.0.0.1:1".to_owned()),
            ("PATH".to_owned(), "/home/u/.krad-ade/b/bin".to_owned()),
        ];
        let (path, rest) = split_path(&env);
        assert_eq!(path, Some("/home/u/.krad-ade/b/bin"));
        assert_eq!(
            rest,
            vec![("KRAD_ADE_ENDPOINT".to_owned(), "127.0.0.1:1".to_owned())]
        );
    }
}
