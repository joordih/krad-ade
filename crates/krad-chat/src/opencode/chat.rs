use krad_domain::{parse_wsl_path, ChatEvent, Effort, ModelId, PermissionMode, ResumeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use super::ask::{reply, Reply};
use super::catalog::{listing, Listing};
use super::events::Translator;
use super::prompt::{outgoing, Known, Outgoing, Place};
use super::server::{Delivery, OpencodeError, OpencodeServer};
use super::tools::{agent, rules, Rule, BUILD_AGENT};
use crate::command::LiveSession;
use crate::control::{AskAnswer, LiveEvent, ToolAsk};
use crate::live::{new_session_id, LiveConfig};
use crate::process::Runner;

const SESSION_PREFIX: &str = "ses";
const SESSION_SEPARATOR: char = '_';
const UUID_SEPARATOR: char = '-';
const MODEL_SEPARATOR: char = '/';
const MISSING_CONVERSATION: &str = "No conversation found with session ID";
const SESSIONS_PATH: &str = "/api/session";

#[derive(Debug)]
pub(crate) enum Order {
    Send(String),
    Interrupt,
    Model(ModelId),
    Effort(Effort),
    Mode(PermissionMode),
    Answer(Box<ToolAsk>, AskAnswer),
}

enum Inbound {
    Server(Delivery),
    Windows(HashMap<String, u64>),
    Emit(LiveEvent),
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct ModelBody {
    #[serde(rename = "providerID")]
    provider_id: String,
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variant: Option<String>,
}

impl ModelBody {
    fn of(model: &ModelId, effort: Option<Effort>) -> Option<Self> {
        let (provider, id) = model.as_str().split_once(MODEL_SEPARATOR)?;
        Some(Self {
            provider_id: provider.to_owned(),
            id: id.to_owned(),
            variant: effort.map(|effort| effort.to_string()),
        })
    }
}

#[derive(Serialize)]
struct Location {
    directory: String,
}

#[derive(Serialize)]
struct CreateBody {
    id: String,
    agent: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<ModelBody>,
    location: Location,
    permissions: Vec<Rule>,
}

#[derive(Serialize)]
struct PermissionsBody {
    permissions: Vec<Rule>,
}

#[derive(Serialize)]
struct AgentBody {
    agent: &'static str,
}

#[derive(Serialize)]
struct ModelSwitch {
    model: ModelBody,
}

#[derive(Serialize)]
struct EnvironmentBody {
    variables: HashMap<String, String>,
}

#[derive(Serialize)]
struct ForkBody {}

#[derive(Deserialize)]
struct SessionInfo {
    id: String,
}

pub(crate) fn opencode_session_id(resume: &ResumeId) -> String {
    let raw = resume.as_str();
    if raw.starts_with(SESSION_PREFIX) {
        return raw.to_owned();
    }
    let digits: String = raw.chars().filter(|ch| *ch != UUID_SEPARATOR).collect();
    format!("{SESSION_PREFIX}{SESSION_SEPARATOR}{digits}")
}

#[must_use]
pub fn new_opencode_session_id() -> ResumeId {
    ResumeId::try_from(opencode_session_id(&new_session_id()))
        .unwrap_or_else(|_| unreachable!("an opencode session id is a valid resume id"))
}

/// # Errors
///
/// Fails when the server cannot read or create the session.
pub fn prepare_opencode_session(
    server: &OpencodeServer,
    resume: &ResumeId,
    directory: &std::path::Path,
    model: Option<&ModelId>,
) -> Result<(), OpencodeError> {
    let session = opencode_session_id(resume);
    let model = model.and_then(|model| ModelBody::of(model, None));
    if server.exists(&session_path(&session, ""))? {
        return match model {
            Some(model) => server.send(&session_path(&session, "/model"), &ModelSwitch { model }),
            None => Ok(()),
        };
    }
    let body = CreateBody {
        id: session,
        agent: BUILD_AGENT,
        model,
        location: Location {
            directory: directory.to_string_lossy().into_owned(),
        },
        permissions: rules(PermissionMode::Manual),
    };
    server
        .post::<_, SessionInfo>(SESSIONS_PATH, &body)
        .map(drop)
}

pub(crate) fn served_directory(runner: &Runner, cwd: &std::path::Path) -> String {
    match runner {
        Runner::Wsl { .. } => parse_wsl_path(cwd).map_or_default(|inside| inside.linux),
        Runner::Native(_) => cwd.to_string_lossy().into_owned(),
    }
}

fn session_path(session: &str, tail: &str) -> String {
    format!("{SESSIONS_PATH}/{session}{tail}")
}

pub(crate) struct OpencodeChat {
    orders: Sender<Order>,
    server: Arc<OpencodeServer>,
    running: Arc<AtomicBool>,
}

impl OpencodeChat {
    pub(crate) fn order(&self, order: Order) {
        self.orders.send(order).unwrap_or_default();
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.running.load(Ordering::SeqCst) && self.server.is_alive()
    }
}

struct Worker {
    server: Arc<OpencodeServer>,
    config: LiveConfig,
    session: String,
    directory: String,
    listing: Listing,
    preamble: Option<String>,
    effort: Option<Effort>,
    model: Option<ModelId>,
    outbox: Sender<Inbound>,
    token: u64,
}

pub(crate) fn spawn_opencode(
    server: Arc<OpencodeServer>,
    config: LiveConfig,
    session: &LiveSession,
    mut sink: impl FnMut(LiveEvent) + Send + 'static,
) -> OpencodeChat {
    let (orders, inbox) = channel();
    let (outbox, inbound) = channel::<Inbound>();
    let running = Arc::new(AtomicBool::new(true));
    let mode = config.permission_mode.unwrap_or(PermissionMode::Manual);
    thread::spawn(move || {
        let mut translator = Translator::new(HashMap::new(), Some(mode));
        for message in inbound {
            match message {
                Inbound::Server(delivery) => {
                    for event in translator.translate(&delivery.event, delivery.nested) {
                        sink(event);
                    }
                }
                Inbound::Windows(windows) => translator = Translator::new(windows, translator.mode),
                Inbound::Emit(event) => sink(event),
            }
        }
    });
    let worker = Worker {
        directory: served_directory(&config.runner, &config.cwd),
        preamble: config.system_prompt.clone(),
        effort: config.effort,
        model: config.model.clone(),
        server: Arc::clone(&server),
        session: String::new(),
        listing: Listing::default(),
        config,
        outbox,
        token: 0,
    };
    let session = session.clone();
    let alive = Arc::clone(&running);
    thread::spawn(move || {
        worker.run(&session, &inbox);
        alive.store(false, Ordering::SeqCst);
    });
    OpencodeChat {
        orders,
        server,
        running,
    }
}

impl Worker {
    fn emit(&self, event: LiveEvent) {
        self.outbox.send(Inbound::Emit(event)).unwrap_or_default();
    }

    fn report(&self, error: &OpencodeError) {
        self.emit(LiveEvent::Chat(ChatEvent::Error(error.to_string())));
    }

    fn mode(&self) -> PermissionMode {
        self.config
            .permission_mode
            .unwrap_or(PermissionMode::Manual)
    }

    fn listen(&mut self, session: &str) {
        let outbox = self.outbox.clone();
        self.token = self.server.subscribe(
            session,
            Box::new(move |delivery| {
                outbox.send(Inbound::Server(delivery)).unwrap_or_default();
            }),
        );
    }

    fn open(&mut self, session: &LiveSession) -> Result<bool, OpencodeError> {
        match session {
            LiveSession::Start(resume) => {
                self.session = opencode_session_id(resume);
                self.listen(&self.session.clone());
                let mode = self.mode();
                let body = CreateBody {
                    id: self.session.clone(),
                    agent: agent(mode),
                    model: self
                        .model
                        .as_ref()
                        .and_then(|model| ModelBody::of(model, self.effort)),
                    location: Location {
                        directory: self.directory.clone(),
                    },
                    permissions: rules(mode),
                };
                self.server.post::<_, SessionInfo>(SESSIONS_PATH, &body)?;
            }
            LiveSession::Resume(resume) => {
                self.session = opencode_session_id(resume);
                if !self.server.exists(&session_path(&self.session, ""))? {
                    self.emit(LiveEvent::Chat(ChatEvent::Error(format!(
                        "{MISSING_CONVERSATION} {}",
                        self.session
                    ))));
                    return Ok(false);
                }
                self.listen(&self.session.clone());
                self.apply_mode(self.mode())?;
                if let Some(model) = self.model.clone() {
                    self.switch_model(&model)?;
                }
            }
            LiveSession::Fork(resume) => {
                let from = opencode_session_id(resume);
                let forked: SessionInfo = self
                    .server
                    .post(&session_path(&from, "/fork"), &ForkBody {})?;
                self.session = forked.id;
                self.listen(&self.session.clone());
                self.apply_mode(self.mode())?;
            }
        }
        if !self.config.bus_vars.0.is_empty() {
            let variables = self.config.bus_vars.0.iter().cloned().collect();
            self.server.put(
                &session_path(&self.session, "/environment"),
                &EnvironmentBody { variables },
            )?;
        }
        if let Ok(resume) = ResumeId::try_from(self.session.clone()) {
            self.emit(LiveEvent::Chat(ChatEvent::Session(resume)));
        }
        self.emit(LiveEvent::Chat(ChatEvent::PermissionMode(self.mode())));
        Ok(true)
    }

    fn apply_mode(&self, mode: PermissionMode) -> Result<(), OpencodeError> {
        self.server.patch(
            &session_path(&self.session, ""),
            &PermissionsBody {
                permissions: rules(mode),
            },
        )?;
        self.server.send(
            &session_path(&self.session, "/agent"),
            &AgentBody { agent: agent(mode) },
        )
    }

    fn switch_model(&self, model: &ModelId) -> Result<(), OpencodeError> {
        let Some(model) = ModelBody::of(model, self.effort) else {
            return Ok(());
        };
        self.server.send(
            &session_path(&self.session, "/model"),
            &ModelSwitch { model },
        )
    }

    fn deliver(&mut self, text: &str) -> Result<(), OpencodeError> {
        let preamble = self.preamble.take();
        let known = Known {
            commands: &self.listing.commands,
            skills: &self.listing.skills,
        };
        let place = Place {
            local: &self.config.cwd,
            served: &self.directory,
        };
        match outgoing(text, preamble.as_deref(), &known, &place) {
            Outgoing::Prompt(body) => self
                .server
                .post::<_, serde_json::Value>(&session_path(&self.session, "/prompt"), &body)
                .map(drop),
            Outgoing::Command(body) => self
                .server
                .send(&session_path(&self.session, "/command"), &body),
        }
    }

    fn handle(&mut self, order: Order) -> Result<(), OpencodeError> {
        match order {
            Order::Send(text) => self.deliver(&text),
            Order::Interrupt => self
                .server
                .nudge(&session_path(&self.session, "/interrupt")),
            Order::Model(model) => {
                self.switch_model(&model)?;
                self.model = Some(model);
                Ok(())
            }
            Order::Effort(effort) => {
                self.effort = Some(effort);
                match self.model.clone() {
                    Some(model) => self.switch_model(&model),
                    None => Ok(()),
                }
            }
            Order::Mode(mode) => {
                self.apply_mode(mode)?;
                self.config.permission_mode = Some(mode);
                self.emit(LiveEvent::Chat(ChatEvent::PermissionMode(mode)));
                Ok(())
            }
            Order::Answer(ask, answer) => match reply(&ask, &answer) {
                Some(Reply::Post { path, body }) => self.server.send(&path, &body),
                Some(Reply::Delete { path }) => self.server.delete(&path),
                None => Ok(()),
            },
        }
    }

    fn run(mut self, session: &LiveSession, inbox: &Receiver<Order>) {
        match self.open(session) {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => return self.report(&error),
        }
        self.listing = listing(&self.server, &self.directory);
        let windows = std::mem::take(&mut self.listing.windows);
        self.outbox
            .send(Inbound::Windows(windows))
            .unwrap_or_default();
        self.emit(LiveEvent::Ready(self.listing.catalog.clone()));
        for order in inbox {
            if let Err(error) = self.handle(order) {
                self.report(&error);
            }
        }
        self.server.unsubscribe(&self.session, self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_resume_id_becomes_an_opencode_session_id() {
        let uuid: ResumeId = "4f2c0b7c-a91e-4b2a-9c1d-0e5f6a7b8c9d"
            .parse()
            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        assert_eq!(
            opencode_session_id(&uuid),
            "ses_4f2c0b7ca91e4b2a9c1d0e5f6a7b8c9d"
        );
        let own: ResumeId = "ses_9f3c1a2b4d5eAbCd"
            .parse()
            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        assert_eq!(opencode_session_id(&own), "ses_9f3c1a2b4d5eAbCd");
    }

    #[test]
    fn a_model_selector_splits_into_provider_and_model_with_its_variant() {
        let model = ModelId::parse("anthropic/claude-sonnet-5-5")
            .unwrap_or_else(|| unreachable!("the fixture has it"));
        assert_eq!(
            ModelBody::of(&model, Some(Effort::High)),
            Some(ModelBody {
                provider_id: "anthropic".to_owned(),
                id: "claude-sonnet-5-5".to_owned(),
                variant: Some("high".to_owned()),
            })
        );
        assert_eq!(
            ModelBody::of(
                &ModelId::parse("opus").unwrap_or_else(|| unreachable!("the fixture has it")),
                None
            ),
            None
        );
    }
}
