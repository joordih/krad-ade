use krad_host::HostLink;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, PipeWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use super::super::process::AgentProcess;
use super::super::{spawn_worker, AcpChat, Launch, Serving};
use crate::acp::client_side::OpenBuffers;
use crate::acp::connection::AcpConnection;
use crate::control::LiveEvent;
use crate::live::{LiveHarness, LiveSession};
use crate::process::Runner;

pub(super) const WAIT: Duration = Duration::from_secs(5);
pub(super) const QUIET: Duration = Duration::from_millis(200);
pub(super) const SESSION: &str = "sess_1";
pub(super) const CWD: &str = "/work";

#[derive(Debug, Default)]
pub(super) struct FakeBuffers {
    pub(super) unsaved: Mutex<HashMap<PathBuf, String>>,
    pub(super) written: Mutex<Vec<(PathBuf, String)>>,
}

impl OpenBuffers for FakeBuffers {
    fn unsaved_text(&self, path: &Path) -> Option<String> {
        self.unsaved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .cloned()
    }

    fn written(&self, path: &Path, content: &str) {
        self.written
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((path.to_path_buf(), content.to_owned()));
    }
}

pub(super) struct FakeAgent {
    lines: Receiver<Value>,
    out: PipeWriter,
}

impl FakeAgent {
    pub(super) fn next(&self) -> Value {
        self.lines
            .recv_timeout(WAIT)
            .unwrap_or_else(|_| unreachable!("the client sent nothing more"))
    }

    pub(super) fn sent_nothing(&self) -> bool {
        self.lines.recv_timeout(QUIET).is_err()
    }

    pub(super) fn expect(&self, method: &str) -> Value {
        let line = self.next();
        assert_eq!(line["method"], method, "unexpected line {line}");
        line
    }

    fn write(&mut self, line: &Value) {
        let mut text = line.to_string();
        text.push('\n');
        self.out
            .write_all(text.as_bytes())
            .and_then(|()| self.out.flush())
            .unwrap_or_else(|error| unreachable!("the client stopped reading: {error}"));
    }

    pub(super) fn reply(&mut self, request: &Value, result: &Value) {
        let id = request["id"].clone();
        self.write(&json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    pub(super) fn fail(&mut self, request: &Value, code: i32, message: &str) {
        let id = request["id"].clone();
        self.write(
            &json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
        );
    }

    pub(super) fn update(&mut self, update: &Value) {
        self.write(&json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {"sessionId": SESSION, "update": update}
        }));
    }

    pub(super) fn ask(&mut self, id: &str, method: &str, params: &Value) {
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
    }

    pub(super) fn initialized(&mut self, capabilities: &Value, auth_methods: &Value) -> Value {
        let initialize = self.expect("initialize");
        self.reply(
            &initialize,
            &json!({"protocolVersion": 1, "agentCapabilities": capabilities, "authMethods": auth_methods}),
        );
        initialize
    }
}

pub(super) struct Started {
    pub(super) chat: AcpChat,
    pub(super) events: Receiver<LiveEvent>,
    pub(super) agent: FakeAgent,
    pub(super) buffers: Arc<FakeBuffers>,
}

impl Started {
    pub(super) fn event(&self) -> LiveEvent {
        self.events
            .recv_timeout(WAIT)
            .unwrap_or_else(|_| unreachable!("the chat emitted nothing more"))
    }

    pub(super) fn until(&self, mut wanted: impl FnMut(&LiveEvent) -> bool) -> Vec<LiveEvent> {
        let mut seen = Vec::new();
        loop {
            let event = self.event();
            let done = wanted(&event);
            seen.push(event);
            if done {
                return seen;
            }
        }
    }

    pub(super) fn ended(&self) -> bool {
        for _ in 0..50 {
            if !self.chat.running.load(Ordering::SeqCst) {
                return true;
            }
            thread::sleep(QUIET / 10);
        }
        false
    }
}

pub(super) fn start(session: &LiveSession) -> Started {
    let pipe = || std::io::pipe().unwrap_or_else(|error| unreachable!("no pipe: {error}"));
    let (agent_in, client_out) = pipe();
    let (client_in, agent_out) = pipe();
    let (lines_sender, lines) = channel();
    thread::spawn(move || {
        for line in BufReader::new(agent_in).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if lines_sender.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let (sender, events) = channel();
    let launch = Launch {
        runner: Runner::Native(PathBuf::from("fake-agent")),
        paths: crate::acp::paths::AgentPaths::on_host(PathBuf::from(CWD)),
        agent: "Fake (ACP)".to_owned(),
        mcp: Vec::new(),
    };
    let buffers = Arc::new(FakeBuffers::default());
    let serving = Serving {
        host: HostLink::Local,
        buffers: Arc::clone(&buffers) as Arc<dyn OpenBuffers>,
    };
    let chat = spawn_worker(
        launch,
        serving,
        session,
        move |event| drop(sender.send(event)),
        Arc::new(AgentProcess::default()),
        move |forward| Ok(AcpConnection::start(client_in, client_out, forward)),
    );
    Started {
        chat,
        events,
        agent: FakeAgent {
            lines,
            out: agent_out,
        },
        buffers,
    }
}

pub(super) fn order(started: &Started, order: crate::live::Order) {
    started.chat.order(order);
}
