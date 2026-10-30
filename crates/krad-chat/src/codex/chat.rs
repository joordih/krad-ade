use krad_domain::{ChatEvent, Effort, PermissionMode, Prompt, ResumeId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use super::ask::{reply, server_ask};
use super::catalog::{listing, Listing};
use super::events::{tools_of, Item, ItemNotice, Translator, ITEM_STARTED};
use super::rpc::{AppServer, CodexError, Pushed};
use crate::control::{LiveEvent, Responder};
use crate::live::LiveConfig;
use crate::live::{LiveHarness, LiveSession, Order};
use crate::mention::served_path;
use crate::refused::ResumeRefused;
use crate::rewind::ResumeCut;

const THREAD_START: &str = "thread/start";
const THREAD_RESUME: &str = "thread/resume";
const THREAD_FORK: &str = "thread/fork";
const TURN_START: &str = "turn/start";
const TURN_STEER: &str = "turn/steer";
const TURN_INTERRUPT: &str = "turn/interrupt";
const SKILL_MARK: char = '/';
const SKILL_SIGIL: char = '$';
const MISSING_THREAD_HINTS: [&str; 3] = ["not found", "no rollout", "does not exist"];
const BUS_NETWORK: &str = "sandbox_workspace_write.network_access=true";
const STOPPED_TEXT: &str = "codex stopped";

enum Inbound {
    Order(Order),
    Pushed(Pushed),
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Sandbox {
    #[serde(rename_all = "camelCase")]
    WorkspaceWrite {
        network_access: bool,
    },
    #[serde(rename_all = "camelCase")]
    ReadOnly {
        network_access: bool,
    },
    DangerFullAccess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SandboxMode {
    WorkspaceWrite,
    ReadOnly,
    DangerFullAccess,
}

impl From<Sandbox> for SandboxMode {
    fn from(sandbox: Sandbox) -> Self {
        match sandbox {
            Sandbox::WorkspaceWrite { .. } => Self::WorkspaceWrite,
            Sandbox::ReadOnly { .. } => Self::ReadOnly,
            Sandbox::DangerFullAccess => Self::DangerFullAccess,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Approval {
    Untrusted,
    OnRequest,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Default,
    Plan,
}

#[derive(Debug, PartialEq, Eq)]
struct Policy {
    approval: Approval,
    sandbox: Sandbox,
    mode: Mode,
}

fn policy(mode: PermissionMode, network: bool) -> Policy {
    let write = Sandbox::WorkspaceWrite {
        network_access: network,
    };
    let (approval, sandbox, mode) = match mode {
        PermissionMode::Manual => (Approval::Untrusted, write, Mode::Default),
        PermissionMode::AcceptEdits => (Approval::OnRequest, write, Mode::Default),
        PermissionMode::Plan => (
            Approval::OnRequest,
            Sandbox::ReadOnly {
                network_access: network,
            },
            Mode::Plan,
        ),
        PermissionMode::Auto | PermissionMode::DontAsk => (Approval::Never, write, Mode::Default),
        PermissionMode::BypassPermissions => {
            (Approval::Never, Sandbox::DangerFullAccess, Mode::Default)
        }
    };
    Policy {
        approval,
        sandbox,
        mode,
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Input<'a> {
    Text {
        text: Cow<'a, str>,
        text_elements: [(); 0],
    },
    Skill {
        name: &'a str,
        path: &'a str,
    },
    LocalImage {
        path: &'a str,
    },
}

fn inputs<'a>(
    text: &'a str,
    skills: &'a HashMap<String, String>,
    images: &'a [String],
) -> Vec<Input<'a>> {
    let mut input = typed_inputs(text, skills);
    if !images.is_empty() {
        input.retain(|input| !matches!(input, Input::Text { text, .. } if text.is_empty()));
    }
    input.extend(images.iter().map(|path| Input::LocalImage { path }));
    input
}

fn typed_inputs<'a>(text: &'a str, skills: &'a HashMap<String, String>) -> Vec<Input<'a>> {
    let text = text.trim();
    let called = text.strip_prefix(SKILL_MARK).and_then(|call| {
        let (name, rest) = call
            .split_once(char::is_whitespace)
            .map_or((call, ""), |(name, rest)| (name, rest.trim()));
        skills
            .get_key_value(name)
            .map(|(name, path)| (name.as_str(), path.as_str(), rest))
    });
    match called {
        Some((name, path, rest)) => vec![
            Input::Text {
                text: Cow::Owned(format!("{SKILL_SIGIL}{name} {rest}").trim_end().to_owned()),
                text_elements: [],
            },
            Input::Skill { name, path },
        ],
        None => vec![Input::Text {
            text: Cow::Borrowed(text),
            text_elements: [],
        }],
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadOpen<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
    cwd: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    approval_policy: Approval,
    sandbox: SandboxMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    developer_instructions: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    exclude_turns: bool,
}

#[derive(Deserialize)]
struct ThreadInfo {
    id: String,
}

#[derive(Deserialize)]
struct ThreadOpened {
    thread: ThreadInfo,
    model: Option<String>,
}

#[derive(Serialize)]
struct CollaborationSettings<'a> {
    model: &'a str,
    reasoning_effort: Option<String>,
    developer_instructions: Option<&'a str>,
}

#[derive(Serialize)]
struct Collaboration<'a> {
    mode: Mode,
    settings: CollaborationSettings<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnStart<'a> {
    thread_id: &'a str,
    input: Vec<Input<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    approval_policy: Approval,
    sandbox_policy: Sandbox,
    #[serde(skip_serializing_if = "Option::is_none")]
    collaboration_mode: Option<Collaboration<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnSteer<'a> {
    thread_id: &'a str,
    expected_turn_id: &'a str,
    input: Vec<Input<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnInterrupt<'a> {
    thread_id: &'a str,
    turn_id: &'a str,
}

fn missing_thread(error: &CodexError) -> bool {
    let CodexError::Refused(message) = error else {
        return false;
    };
    let message = message.to_lowercase();
    MISSING_THREAD_HINTS
        .iter()
        .any(|hint| message.contains(hint))
}

pub(crate) struct CodexChat {
    orders: Sender<Inbound>,
    running: Arc<AtomicBool>,
}

impl LiveHarness for CodexChat {
    fn order(&self, order: Order) {
        self.orders.send(Inbound::Order(order)).unwrap_or_default();
    }

    fn is_alive(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop(&self) {
        self.orders.send(Inbound::Stop).unwrap_or_default();
    }
}

struct Worker<S> {
    config: LiveConfig,
    sink: S,
    directory: String,
    thread: String,
    model: Option<String>,
    effort: Option<Effort>,
    mode: PermissionMode,
    network: bool,
    listing: Listing,
    translator: Translator,
    changes: HashMap<String, Vec<krad_domain::ToolCall>>,
}

pub(crate) fn spawn_codex(
    config: LiveConfig,
    session: &LiveSession,
    sink: impl FnMut(LiveEvent) + Send + 'static,
) -> CodexChat {
    let (orders, inbox) = channel();
    let running = Arc::new(AtomicBool::new(true));
    let alive = Arc::clone(&running);
    let session = session.clone();
    let pushed = orders.clone();
    thread::spawn(move || {
        let network = !config.bus_vars.0.is_empty();
        let mut worker = Worker {
            directory: config.runner.served_directory(&config.cwd),
            model: config.model.as_ref().map(|model| model.as_str().to_owned()),
            effort: config.effort,
            mode: config.permission_mode.unwrap_or(PermissionMode::Manual),
            thread: String::new(),
            listing: Listing::default(),
            translator: Translator::default(),
            changes: HashMap::new(),
            network,
            config,
            sink,
        };
        worker.run(&session, pushed, &inbox);
        alive.store(false, Ordering::SeqCst);
    });
    CodexChat { orders, running }
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    fn emit(&mut self, event: ChatEvent) {
        (self.sink)(LiveEvent::Chat(event));
    }

    fn start_server(&self, inbound: Sender<Inbound>) -> Result<AppServer, CodexError> {
        let (pushed, incoming) = channel();
        thread::spawn(move || {
            for push in incoming {
                if inbound.send(Inbound::Pushed(push)).is_err() {
                    break;
                }
            }
        });
        let overrides: Vec<String> = self
            .network
            .then(|| BUS_NETWORK.to_owned())
            .into_iter()
            .collect();
        AppServer::start(
            &self.config.runner,
            &self.config.cwd,
            &overrides,
            &self.config.bus_vars.0,
            pushed,
        )
    }

    fn open(&mut self, server: &AppServer, session: &LiveSession) -> Result<bool, CodexError> {
        let policy = policy(self.mode, self.network);
        let instructions = self.config.system_prompt.clone();
        let opening = |thread_id| ThreadOpen {
            thread_id,
            cwd: &self.directory,
            model: self.model.as_deref(),
            approval_policy: policy.approval,
            sandbox: policy.sandbox.into(),
            developer_instructions: instructions.as_deref(),
            exclude_turns: thread_id.is_some(),
        };
        let opened: Result<ThreadOpened, CodexError> = match session {
            LiveSession::Start(_) => server.call(THREAD_START, &opening(None)),
            LiveSession::Resume(resume) | LiveSession::Cut(ResumeCut { from: resume, .. }) => {
                server.call(THREAD_RESUME, &opening(Some(resume.as_str())))
            }
            LiveSession::Fork(resume) => server.call(THREAD_FORK, &opening(Some(resume.as_str()))),
        };
        let opened = match opened {
            Ok(opened) => opened,
            Err(error) if missing_thread(&error) => {
                let resumed = session.history().map_or("", ResumeId::as_str);
                (self.sink)(LiveEvent::Refused(ResumeRefused::missing(resumed)));
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        self.thread = opened.thread.id;
        if let Ok(resume) = ResumeId::try_from(self.thread.clone()) {
            self.emit(ChatEvent::Session(resume));
        }
        if let Some(model) = opened.model {
            self.translator.model = Some(model.clone());
            self.model.get_or_insert(model.clone());
            self.emit(ChatEvent::Model(model));
        }
        let mode = self.mode;
        self.emit(ChatEvent::PermissionMode(mode));
        Ok(true)
    }

    fn deliver(&mut self, server: &AppServer, prompt: &Prompt) -> Result<(), CodexError> {
        let runner = &self.config.runner;
        let text = prompt.text_with_paths(|path| served_path(runner, path));
        let images: Vec<String> = prompt
            .images()
            .map(|image| served_path(runner, &image.path))
            .collect();
        let input = inputs(&text, &self.listing.skills, &images);
        if let Some(turn) = self.translator.turn.as_deref() {
            return server
                .call::<_, Value>(
                    TURN_STEER,
                    &TurnSteer {
                        thread_id: &self.thread,
                        expected_turn_id: turn,
                        input,
                    },
                )
                .map(drop);
        }
        let policy = policy(self.mode, self.network);
        let model = self.model.as_deref().or(self.translator.model.as_deref());
        let effort = self.effort.map(|effort| effort.to_string());
        let collaboration_mode = model.map(|model| Collaboration {
            mode: policy.mode,
            settings: CollaborationSettings {
                model,
                reasoning_effort: effort.clone(),
                developer_instructions: None,
            },
        });
        server
            .call::<_, Value>(
                TURN_START,
                &TurnStart {
                    thread_id: &self.thread,
                    input,
                    model,
                    effort,
                    approval_policy: policy.approval,
                    sandbox_policy: policy.sandbox,
                    collaboration_mode,
                },
            )
            .map(drop)
    }

    fn handle(&mut self, server: &AppServer, order: Order) -> Result<(), CodexError> {
        match order {
            Order::Send(prompt) => return self.deliver(server, &prompt),
            Order::Interrupt => {
                if let Some(turn) = self.translator.turn.clone() {
                    server.call::<_, Value>(
                        TURN_INTERRUPT,
                        &TurnInterrupt {
                            thread_id: &self.thread,
                            turn_id: &turn,
                        },
                    )?;
                }
            }
            Order::Model(model) => {
                self.model = Some(model.as_str().to_owned());
                self.emit(ChatEvent::Model(model.as_str().to_owned()));
            }
            Order::Effort(effort) => self.effort = Some(effort),
            Order::Mode(mode) => {
                self.mode = mode;
                self.emit(ChatEvent::PermissionMode(mode));
            }
            Order::Answer(ask, answer) => {
                let Responder::Codex { id, ask } = ask.responder else {
                    return Ok(());
                };
                match reply(&ask, &answer) {
                    Some(body) => server.answer(&id, &body),
                    None => server.refuse(&id),
                }
            }
            Order::Rewind(request) => {
                let outcome = self.rewind(server, &request);
                (self.sink)(LiveEvent::Rewound(outcome));
            }
        }
        Ok(())
    }

    fn pushed(&mut self, server: &AppServer, push: Pushed) {
        match push {
            Pushed::Notification { method, params } => {
                if method == ITEM_STARTED {
                    if let Ok(ItemNotice {
                        item: item @ Item::FileChange { .. },
                    }) = ItemNotice::deserialize(&params)
                    {
                        if let Item::FileChange { id, .. } = &item {
                            self.changes.insert(id.clone(), tools_of(&item));
                        }
                    }
                }
                for event in self.translator.translate(&method, &params) {
                    (self.sink)(event);
                }
            }
            Pushed::Request { id, method, params } => {
                match server_ask(id.clone(), &method, &params, &self.changes) {
                    Some(ask) => (self.sink)(LiveEvent::Ask(Box::new(ask))),
                    None => server.refuse(&id),
                }
            }
            Pushed::Closed => {}
        }
    }

    fn run(&mut self, session: &LiveSession, inbound: Sender<Inbound>, inbox: &Receiver<Inbound>) {
        let server = match self.start_server(inbound) {
            Ok(server) => server,
            Err(error) => return self.emit(ChatEvent::Error(error.to_string())),
        };
        match self.open(&server, session) {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => return self.emit(ChatEvent::Error(error.to_string())),
        }
        self.listing = listing(&server, &self.directory);
        (self.sink)(LiveEvent::Ready(self.listing.catalog.clone()));
        for inbound in inbox {
            match inbound {
                Inbound::Order(order) => {
                    if let Err(error) = self.handle(&server, order) {
                        self.emit(ChatEvent::Error(error.to_string()));
                    }
                }
                Inbound::Pushed(Pushed::Closed) => {
                    let detail = server.stderr();
                    self.emit(ChatEvent::Error(format!("{STOPPED_TEXT}: {detail}")));
                    return;
                }
                Inbound::Pushed(push) => self.pushed(&server, push),
                Inbound::Stop => return,
            }
        }
    }
}

mod rewind;
#[cfg(test)]
mod tests;
