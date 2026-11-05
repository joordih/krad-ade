mod choosing;

use krad_domain::{AcpModelId, ChatEvent, Prompt, ResumeId};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

use super::asks::Asks;
use super::choices::Choices;
use super::client_side::serve_client_side;
use super::opening::{
    auth_methods, initialize_request, open, opening_for, OpenFailure, Opened, Opening,
    SETUP_TIMEOUT,
};
use super::process::AgentProcess;
use super::prompt::prompt_blocks;
use super::translate::Translator;
use super::{settle_later, Answered, Inbound, Launch};
use crate::acp::connection::{AcpConnection, AcpError};
use crate::acp::ids::SessionId;
use crate::acp::prompt::{CancelNotification, PromptRequest, PromptResponse, StopReason};
use crate::acp::setup::{AgentCapabilities, AuthMethod, ProtocolVersion};
use crate::acp::update::{SessionNotification, SessionUpdate};
use crate::control::{AcpEvent, LiveEvent};
use crate::live::{AcpOrder, LiveSession, Order};
use crate::refused::ResumeRefused;
use crate::rewind::RewindError;

const SIGN_IN_TIMEOUT: Duration = Duration::from_mins(10);
const STOPPED_TEXT: &str = "the agent stopped";
const UNSUPPORTED_VERSION_TEXT: &str = "the agent speaks an ACP version the app does not know:";
const NO_SIGN_IN_TEXT: &str = "the agent asks to sign in but offers no way to";
const REFUSAL_TEXT: &str = "the agent refused to go on with this request";
const MAX_TOKENS_TEXT: &str = "the agent stopped at its token limit";
const MAX_TURNS_TEXT: &str = "the agent stopped at its limit of model requests for one turn";
const SIGN_IN_FAILED_TEXT: &str = "sign-in failed";
const CHOICE_REFUSED_TEXT: &str = "the agent did not take the change";
const SKIPPED_SERVERS_TEXT: &str = "MCP servers left out for this agent:";
const LIST_SEPARATOR: &str = ", ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    Go,
    Stop,
}

pub(super) struct Worker<S> {
    sink: S,
    launch: Launch,
    connection: AcpConnection,
    inbound: Sender<Inbound>,
    process: Arc<AgentProcess>,
    running: Arc<AtomicBool>,
    capabilities: AgentCapabilities,
    methods: Vec<AuthMethod>,
    session: Option<SessionId>,
    signing_in: Option<Opening>,
    choices: Choices,
    translator: Translator,
    asks: Asks,
    queue: VecDeque<Prompt>,
    prompting: bool,
    replaying: bool,
}

const fn is_replayed(update: &SessionUpdate) -> bool {
    matches!(
        update,
        SessionUpdate::UserMessageChunk(_)
            | SessionUpdate::AgentMessageChunk(_)
            | SessionUpdate::AgentThoughtChunk(_)
            | SessionUpdate::ToolCall(_)
            | SessionUpdate::ToolCallUpdate(_)
            | SessionUpdate::Plan(_)
    )
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn new(
        launch: Launch,
        connection: AcpConnection,
        inbound: Sender<Inbound>,
        process: Arc<AgentProcess>,
        running: Arc<AtomicBool>,
        sink: S,
    ) -> Self {
        Self {
            sink,
            launch,
            connection,
            inbound,
            process,
            running,
            capabilities: AgentCapabilities::default(),
            methods: Vec::new(),
            session: None,
            signing_in: None,
            choices: Choices::default(),
            translator: Translator::default(),
            asks: Asks::default(),
            queue: VecDeque::new(),
            prompting: false,
            replaying: false,
        }
    }

    fn emit(&mut self, event: ChatEvent) {
        (self.sink)(LiveEvent::Chat(event));
    }

    fn notice(&mut self, text: String) {
        (self.sink)(LiveEvent::Notice(text));
    }

    fn failure_text(&self, error: &AcpError) -> String {
        if !matches!(error, AcpError::Gone) {
            return error.to_string();
        }
        let detail = self.process.stderr();
        if detail.is_empty() {
            STOPPED_TEXT.to_owned()
        } else {
            format!("{STOPPED_TEXT}: {detail}")
        }
    }

    #[tracing::instrument(skip_all, fields(agent = %self.launch.agent))]
    pub(super) fn run(mut self, session: &LiveSession, inbox: &Receiver<Inbound>) {
        if self.initialize() == Flow::Go {
            let flow = match opening_for(session, &self.launch.agent, self.capabilities) {
                Ok(opening) => self.open(opening),
                Err(refused) => self.refuse(refused),
            };
            if flow == Flow::Go {
                for inbound in inbox {
                    if self.handle(inbound) == Flow::Stop {
                        break;
                    }
                }
            }
        }
        self.asks.cancel_all();
        self.process.stop();
        info!("ACP chat ended");
    }

    fn initialize(&mut self) -> Flow {
        match self
            .connection
            .initialize(&initialize_request())
            .wait_timeout(SETUP_TIMEOUT)
        {
            Ok(agent) if agent.protocol_version == ProtocolVersion::V1 => {
                self.capabilities = agent.agent_capabilities;
                self.methods = agent.auth_methods;
                Flow::Go
            }
            Ok(agent) => {
                let version = agent.protocol_version.0;
                self.emit(ChatEvent::Error(format!(
                    "{UNSUPPORTED_VERSION_TEXT} {version}"
                )));
                Flow::Stop
            }
            Err(error) => {
                let text = self.failure_text(&error);
                self.emit(ChatEvent::Error(text));
                Flow::Stop
            }
        }
    }

    fn refuse(&mut self, refused: ResumeRefused) -> Flow {
        self.running.store(false, Ordering::SeqCst);
        (self.sink)(LiveEvent::Refused(refused));
        Flow::Stop
    }

    fn open(&mut self, opening: Opening) -> Flow {
        match open(&self.connection, &self.launch, self.capabilities, &opening) {
            Ok(opened) => {
                self.started(opened);
                Flow::Go
            }
            Err(OpenFailure::SignIn) if self.methods.is_empty() => {
                self.emit(ChatEvent::Error(NO_SIGN_IN_TEXT.to_owned()));
                Flow::Stop
            }
            Err(OpenFailure::SignIn) => {
                self.signing_in = Some(opening);
                let methods = auth_methods(&self.methods);
                (self.sink)(LiveEvent::Acp(AcpEvent::SignInNeeded(methods)));
                Flow::Go
            }
            Err(OpenFailure::Refused(refused)) => self.refuse(refused),
            Err(OpenFailure::Failed(error)) => {
                let text = self.failure_text(&error);
                self.emit(ChatEvent::Error(text));
                Flow::Stop
            }
        }
    }

    fn started(&mut self, opened: Opened) {
        info!(loaded = opened.loaded, "ACP session opened");
        if let Ok(resume) = ResumeId::try_from(opened.id.0.clone()) {
            self.emit(ChatEvent::Session(resume));
        } else {
            debug!("the ACP session id cannot be kept as a resume id");
        }
        self.session = Some(opened.id);
        self.choices = opened.choices;
        self.choices_changed();
        if !opened.skipped_servers.is_empty() {
            let skipped = opened.skipped_servers.join(LIST_SEPARATOR);
            self.notice(format!("{SKIPPED_SERVERS_TEXT} {skipped}"));
        }
        if opened.loaded {
            self.replaying = true;
            if self.inbound.send(Inbound::Loaded).is_err() {
                self.replaying = false;
            }
        }
        (self.sink)(LiveEvent::Ready(self.translator.catalog().clone()));
        self.next_prompt();
    }

    fn choices_changed(&mut self) {
        if let Some(modes) = &self.choices.modes {
            (self.sink)(LiveEvent::Acp(AcpEvent::Modes(modes.offer.clone())));
        }
        let others = self.choices.others.clone();
        (self.sink)(LiveEvent::Acp(AcpEvent::Options(others)));
        if let Some(models) = &self.choices.models {
            let models = models.offer.clone();
            let shown = models
                .current_model()
                .map_or_else(|| models.current.0.clone(), |model| model.name.clone());
            (self.sink)(LiveEvent::Acp(AcpEvent::Models(models)));
            self.emit(ChatEvent::Model(shown));
        }
    }

    fn handle(&mut self, inbound: Inbound) -> Flow {
        match inbound {
            Inbound::Order(order) => return self.order(order),
            Inbound::Update(notification) => self.updated(notification),
            Inbound::Asked(request) => {
                if let Some((asked, reply)) = serve_client_side(request) {
                    let tool = self.translator.asked_tool(&asked.tool_call);
                    let ask = self.asks.asked(tool, asked.options, reply);
                    (self.sink)(LiveEvent::Ask(Box::new(ask)));
                }
            }
            Inbound::Answered(answered) => self.answered(answered),
            Inbound::Loaded => self.replaying = false,
            Inbound::Closed => {
                let text = self.failure_text(&AcpError::Gone);
                self.emit(ChatEvent::Error(text));
                return Flow::Stop;
            }
            Inbound::Stop => return Flow::Stop,
        }
        Flow::Go
    }

    fn updated(&mut self, notification: SessionNotification) {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session != &notification.session_id)
        {
            debug!("skipped an ACP update for another session");
            return;
        }
        if self.replaying && is_replayed(&notification.update) {
            return;
        }
        match notification.update {
            SessionUpdate::CurrentModeUpdate(update) => {
                if let Some(modes) = self.choices.mode_is(update.current_mode_id) {
                    let modes = modes.clone();
                    (self.sink)(LiveEvent::Acp(AcpEvent::Modes(modes)));
                }
            }
            SessionUpdate::ConfigOptionUpdate(update) => {
                self.choices.configure(&update.config_options);
                self.choices_changed();
            }
            update => {
                for event in self.translator.translate(update) {
                    (self.sink)(event);
                }
            }
        }
    }

    fn order(&mut self, order: Order) -> Flow {
        match order {
            Order::Send(prompt) => {
                self.queue.push_back(prompt);
                self.next_prompt();
            }
            Order::Interrupt => self.interrupt(),
            Order::Model(model) => self.choose_model(AcpModelId(model.as_str().to_owned())),
            Order::Acp(AcpOrder::Model(model)) => self.choose_model(model),
            Order::Acp(AcpOrder::Mode(mode)) => self.choose_mode(mode),
            Order::Acp(AcpOrder::Option(option, value)) => self.choose_option(option, value),
            Order::Acp(AcpOrder::SignIn(method)) => return self.sign_in(method),
            Order::Answer(ask, answer) => self.asks.answer(&ask, &answer),
            Order::Rewind(_) => (self.sink)(LiveEvent::Rewound(Err(RewindError::Unsupported))),
            Order::Effort(_) | Order::Mode(_) => {
                debug!("an ACP agent keeps its own modes and efforts");
            }
        }
        Flow::Go
    }

    fn interrupt(&mut self) {
        let Some(session) = self.session.clone().filter(|_| self.prompting) else {
            return;
        };
        self.connection.cancel(&CancelNotification {
            session_id: session,
        });
        self.asks.cancel_all();
    }

    fn next_prompt(&mut self) {
        while !self.prompting && self.signing_in.is_none() {
            let Some(session) = self.session.clone() else {
                return;
            };
            let Some(prompt) = self.queue.pop_front() else {
                return;
            };
            let capabilities = self.capabilities.prompt_capabilities;
            match prompt_blocks(&prompt, &self.launch.runner, capabilities) {
                Ok(blocks) => {
                    self.prompting = true;
                    let pending = self.connection.prompt(&PromptRequest {
                        session_id: session,
                        prompt: blocks,
                    });
                    settle_later(pending, &self.inbound, Answered::Prompted);
                }
                Err(error) => self.emit(ChatEvent::Error(error.to_string())),
            }
        }
    }

    fn answered(&mut self, answered: Answered) {
        match answered {
            Answered::Prompted(outcome) => self.prompted(outcome),
            Answered::ModeSet(mode, Ok(_)) => {
                if let Some(modes) = self.choices.mode_is(mode) {
                    let modes = modes.clone();
                    (self.sink)(LiveEvent::Acp(AcpEvent::Modes(modes)));
                }
            }
            Answered::ModelSet(model, Ok(_)) => {
                if self.choices.model_is(model).is_some() {
                    self.choices_changed();
                }
            }
            Answered::Configured(Ok(configured)) => {
                self.choices.configure(&configured.config_options);
                self.choices_changed();
            }
            Answered::ModeSet(_, Err(error))
            | Answered::ModelSet(_, Err(error))
            | Answered::Configured(Err(error)) => {
                let text = self.failure_text(&error);
                self.notice(format!("{CHOICE_REFUSED_TEXT}: {text}"));
            }
        }
    }

    fn prompted(&mut self, outcome: Result<PromptResponse, AcpError>) {
        self.prompting = false;
        self.asks.cancel_all();
        for event in self.translator.turn_ended() {
            (self.sink)(event);
        }
        match outcome {
            Ok(response) => match response.stop_reason {
                StopReason::EndTurn | StopReason::Cancelled => self.emit(ChatEvent::Done),
                StopReason::MaxTokens => self.ended_early(MAX_TOKENS_TEXT),
                StopReason::MaxTurnRequests => self.ended_early(MAX_TURNS_TEXT),
                StopReason::Refusal => self.emit(ChatEvent::Error(REFUSAL_TEXT.to_owned())),
            },
            Err(AcpError::Gone) => return,
            Err(error) => self.emit(ChatEvent::Error(error.to_string())),
        }
        self.next_prompt();
    }

    fn ended_early(&mut self, text: &str) {
        self.notice(text.to_owned());
        self.emit(ChatEvent::Done);
    }
}
