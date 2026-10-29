use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;
use thiserror::Error;

use crate::control::json_line;
use crate::live::keep_stderr_tail;
use crate::process::{harness_command, Runner};

const APP_SERVER_ARGS: [&str; 1] = ["app-server"];
const CONFIG_FLAG: &str = "-c";
const JSONRPC_VERSION: &str = "2.0";
const INITIALIZE: &str = "initialize";
const INITIALIZED: &str = "initialized";
const CLIENT_NAME: &str = "krad_ade";
const CLIENT_TITLE: &str = "Krad ADE";
const REPLY_TIMEOUT: Duration = Duration::from_mins(2);
const UNSUPPORTED_CODE: i64 = -32601;
const UNSUPPORTED_TEXT: &str = "Krad ADE does not handle this request";

#[derive(Debug, Error)]
pub enum CodexError {
    #[error("cannot run codex: {0}")]
    Io(#[from] std::io::Error),
    #[error("codex refused the request: {0}")]
    Refused(String),
    #[error("codex stopped answering")]
    Gone,
    #[error("codex sent an answer the app cannot read: {0}")]
    Shape(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum RpcId {
    Number(i64),
    Text(String),
}

#[derive(Debug, Deserialize)]
struct RpcFailure {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Incoming {
    Request {
        id: RpcId,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Notification {
        method: String,
        #[serde(default)]
        params: Value,
    },
    Failure {
        id: RpcId,
        error: RpcFailure,
    },
    Success {
        id: RpcId,
        #[serde(default)]
        result: Value,
    },
}

#[derive(Debug)]
pub(crate) enum Pushed {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: RpcId,
        method: String,
        params: Value,
    },
    Closed,
}

#[derive(Serialize)]
struct Call<'a, P> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: P,
}

#[derive(Serialize)]
struct Notice<'a> {
    jsonrpc: &'static str,
    method: &'a str,
}

#[derive(Serialize)]
struct Reply<'a, R> {
    jsonrpc: &'static str,
    id: &'a RpcId,
    result: R,
}

#[derive(Serialize)]
struct ReplyError<'a> {
    jsonrpc: &'static str,
    id: &'a RpcId,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: i64,
    message: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientInfo {
    name: &'static str,
    title: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    experimental_api: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeParams {
    client_info: ClientInfo,
    capabilities: Capabilities,
}

type Waiters = Mutex<HashMap<u64, Sender<Result<Value, String>>>>;

pub(crate) struct AppServer {
    outbox: Sender<String>,
    waiters: Arc<Waiters>,
    next_id: AtomicU64,
    child: Arc<Mutex<Option<Child>>>,
    stderr_tail: Arc<Mutex<String>>,
}

impl AppServer {
    pub(crate) fn start(
        runner: &Runner,
        cwd: &Path,
        overrides: &[String],
        env: &[(String, String)],
        pushed: Sender<Pushed>,
    ) -> Result<Self, CodexError> {
        let mut args: Vec<String> = overrides
            .iter()
            .flat_map(|value| [CONFIG_FLAG.to_owned(), value.clone()])
            .collect();
        args.extend(APP_SERVER_ARGS.iter().map(|arg| (*arg).to_owned()));
        let mut child = harness_command(runner, cwd, &args, env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let tail = Arc::clone(&stderr_tail);
            thread::spawn(move || keep_stderr_tail(stderr, &tail));
        }
        let (outbox, lines) = channel::<String>();
        if let Some(stdin) = stdin {
            thread::spawn(move || write_lines(stdin, &lines));
        }
        let waiters: Arc<Waiters> = Arc::new(Mutex::new(HashMap::new()));
        if let Some(stdout) = stdout {
            let waiters = Arc::clone(&waiters);
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    route(&line, &waiters, &pushed);
                }
                waiters
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clear();
                pushed.send(Pushed::Closed).unwrap_or_default();
            });
        }
        let server = Self {
            outbox,
            waiters,
            next_id: AtomicU64::new(0),
            child: Arc::new(Mutex::new(Some(child))),
            stderr_tail,
        };
        server.call::<_, Value>(
            INITIALIZE,
            &InitializeParams {
                client_info: ClientInfo {
                    name: CLIENT_NAME,
                    title: CLIENT_TITLE,
                    version: env!("CARGO_PKG_VERSION"),
                },
                capabilities: Capabilities {
                    experimental_api: true,
                },
            },
        )?;
        server.write(&Notice {
            jsonrpc: JSONRPC_VERSION,
            method: INITIALIZED,
        });
        Ok(server)
    }

    fn write(&self, message: &impl Serialize) {
        self.outbox.send(json_line(message)).unwrap_or_default();
    }

    pub(crate) fn call<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
    ) -> Result<R, CodexError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = channel();
        self.waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, sender);
        self.write(&Call {
            jsonrpc: JSONRPC_VERSION,
            id,
            method,
            params,
        });
        let answer = receiver
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| CodexError::Gone)?;
        self.waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        let value = answer.map_err(CodexError::Refused)?;
        Ok(serde_json::from_value(value)?)
    }

    pub(crate) fn answer(&self, id: &RpcId, result: &impl Serialize) {
        self.write(&Reply {
            jsonrpc: JSONRPC_VERSION,
            id,
            result,
        });
    }

    pub(crate) fn refuse(&self, id: &RpcId) {
        self.write(&ReplyError {
            jsonrpc: JSONRPC_VERSION,
            id,
            error: ErrorBody {
                code: UNSUPPORTED_CODE,
                message: UNSUPPORTED_TEXT,
            },
        });
    }

    pub(crate) fn stderr(&self) -> String {
        self.stderr_tail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .trim()
            .to_owned()
    }

    pub(crate) fn stop(&self) {
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(mut child) = child.take() {
            child.kill().unwrap_or_default();
            drop(child.wait());
        }
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn route(line: &str, waiters: &Waiters, pushed: &Sender<Pushed>) {
    let Ok(incoming) = serde_json::from_str::<Incoming>(line.trim()) else {
        return;
    };
    let settle = |id: RpcId, answer: Result<Value, String>| {
        let RpcId::Number(id) = id else {
            return;
        };
        let Ok(id) = u64::try_from(id) else {
            return;
        };
        let waiter = waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        if let Some(waiter) = waiter {
            waiter.send(answer).unwrap_or_default();
        }
    };
    match incoming {
        Incoming::Request { id, method, params } => {
            pushed
                .send(Pushed::Request { id, method, params })
                .unwrap_or_default();
        }
        Incoming::Notification { method, params } => {
            pushed
                .send(Pushed::Notification { method, params })
                .unwrap_or_default();
        }
        Incoming::Failure { id, error } => settle(id, Err(error.message)),
        Incoming::Success { id, result } => settle(id, Ok(result)),
    }
}

fn write_lines(mut stdin: ChildStdin, lines: &Receiver<String>) {
    while let Ok(line) = lines.recv() {
        if stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_notifications_and_answers_go_where_they_belong() {
        let waiters: Waiters = Mutex::new(HashMap::new());
        let (answered, answer) = channel();
        waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(3, answered);
        let (pushed, received) = channel();
        route(r#"{"id":3,"result":{"ok":true}}"#, &waiters, &pushed);
        route(
            r#"{"method":"item/commandExecution/requestApproval","id":0,"params":{"command":"ls"}}"#,
            &waiters,
            &pushed,
        );
        route(
            r#"{"method":"turn/started","params":{"threadId":"t"}}"#,
            &waiters,
            &pushed,
        );
        route("not json", &waiters, &pushed);
        assert_eq!(
            answer.try_recv().ok(),
            Some(Ok(serde_json::json!({"ok": true})))
        );
        let pushed: Vec<String> = received
            .try_iter()
            .map(|message| match message {
                Pushed::Request { id, method, .. } => format!("{method} {id:?}"),
                Pushed::Notification { method, .. } => method,
                Pushed::Closed => String::new(),
            })
            .collect();
        assert_eq!(
            pushed,
            [
                "item/commandExecution/requestApproval Number(0)",
                "turn/started"
            ]
        );
    }

    #[test]
    fn a_refused_call_carries_the_reason() {
        let waiters: Waiters = Mutex::new(HashMap::new());
        let (answered, answer) = channel();
        waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(1, answered);
        let (pushed, _) = channel();
        route(
            r#"{"id":1,"error":{"code":-32600,"message":"thread not found"}}"#,
            &waiters,
            &pushed,
        );
        assert_eq!(
            answer.try_recv().ok(),
            Some(Err("thread not found".to_owned()))
        );
    }
}
