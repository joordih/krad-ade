use krad_domain::{
    AgentPreset, ChatEvent, Prompt, ResumeId, RewindId, RewindPoint, UsageBudget, UsageCadence,
};
use std::io::Write;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::{keep_stderr_tail, new_uuid, LiveConfig, LiveHarness, Order};
use crate::command::live_chat_command;
use crate::control::{
    answer_line, initialize_line, interrupt_line, rewind_files_line, set_model_line,
    set_permission_mode_line, usage_line, LiveEvent, Responder,
};
use crate::error::ChatError;
use crate::exit::closing_event;
use crate::history::claude_conversation_file;
use crate::live::LiveSession;
use crate::message::{user_message_line, SlashCommand};
use crate::process::{spawn_harness, Harness};
use crate::refused::ResumeRefused;
use crate::rewind::{transcript_cut, RewindError, RewindRequest, Rewound};

mod routing;

pub(super) use routing::Routing;

const REQUEST_ID_PREFIX: &str = "krad_";
const USAGE_REQUEST_PREFIX: &str = "krad_usage_";
const CHECKPOINTING_VAR: &str = "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING";
const CHECKPOINTING_ON: &str = "true";
const FILES_REPLY_WAIT: Duration = Duration::from_secs(60);
const MISSING_CONVERSATION_TEXT: &str = "No conversation found";
const DROPS_TURN_REFUSED_TEXT: &str = "Resume rejected by --resume-drops-turn";

enum Outbound {
    Line(String),
    Prompt(Prompt, Option<RewindId>),
}

impl Outbound {
    fn into_line(self) -> String {
        match self {
            Self::Line(line) => line,
            Self::Prompt(prompt, uuid) => user_message_line(&prompt, uuid.as_ref()),
        }
    }
}

#[derive(Default)]
pub(super) struct UsageAsk {
    outbox: Option<Sender<Outbound>>,
    next: AtomicU64,
    budget: Arc<UsageBudget>,
}

impl UsageAsk {
    fn new(outbox: Sender<Outbound>, budget: Arc<UsageBudget>) -> Self {
        Self {
            outbox: Some(outbox),
            next: AtomicU64::new(0),
            budget,
        }
    }

    fn ask(&self) {
        let Some(outbox) = &self.outbox else {
            return;
        };
        let cadence = UsageCadence::of(AgentPreset::ClaudeCode);
        if !self.budget.try_spend(Instant::now(), cadence) {
            tracing::trace!("plan usage asked within the shared budget gap; skipped");
            return;
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let line = usage_line(&format!("{USAGE_REQUEST_PREFIX}{id}"));
        outbox.send(Outbound::Line(line)).unwrap_or_default();
    }

    fn answers(request_id: &str) -> bool {
        request_id.starts_with(USAGE_REQUEST_PREFIX)
    }
}

struct Awaited {
    request_id: String,
    reply: Sender<Result<(), RewindError>>,
}

type AwaitedReply = Arc<Mutex<Option<Awaited>>>;
type KnownSession = Arc<Mutex<Option<ResumeId>>>;

pub(super) struct ClaudeProcess {
    outbox: Sender<Outbound>,
    events: Sender<LiveEvent>,
    child: Arc<Mutex<Option<Child>>>,
    pending: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    next_request: AtomicU64,
    session: KnownSession,
    awaited: AwaitedReply,
    host: krad_host::HostLink,
}

impl ClaudeProcess {
    fn request_id(&self) -> String {
        let id = self.next_request.fetch_add(1, Ordering::SeqCst);
        format!("{REQUEST_ID_PREFIX}{id}")
    }

    fn write(&self, line: String) {
        self.outbox.send(Outbound::Line(line)).unwrap_or_default();
    }

    fn queue(&self, prompt: Prompt, uuid: Option<RewindId>) -> bool {
        self.pending.fetch_add(1, Ordering::SeqCst);
        let sent = self.outbox.send(Outbound::Prompt(prompt, uuid)).is_ok();
        if !sent {
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
        sent
    }

    fn send(&self, prompt: Prompt) {
        let uuid = RewindId::from(new_uuid());
        if self.queue(prompt, Some(uuid.clone())) {
            let point = ChatEvent::Prompted(RewindPoint::Message(uuid));
            self.events.send(LiveEvent::Chat(point)).unwrap_or_default();
        }
    }

    fn rewind(&self, request: RewindRequest) {
        let job = RewindJob {
            session: self
                .session
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            request_id: self.request_id(),
            outbox: self.outbox.clone(),
            awaited: Arc::clone(&self.awaited),
            host: self.host.clone(),
        };
        let events = self.events.clone();
        thread::spawn(move || {
            let outcome = job.run(&request);
            events.send(LiveEvent::Rewound(outcome)).unwrap_or_default();
        });
    }
}

impl LiveHarness for ClaudeProcess {
    fn order(&self, order: Order) {
        match order {
            Order::Send(prompt) => self.send(prompt),
            Order::Interrupt => self.write(interrupt_line(&self.request_id())),
            Order::Model(model) => self.write(set_model_line(&self.request_id(), &model)),
            Order::Effort(effort) => self.run(&SlashCommand::Effort(effort)),
            Order::Mode(mode) => self.write(set_permission_mode_line(&self.request_id(), mode)),
            Order::Answer(ask, answer) => {
                if let Responder::ClaudeCode(asked) = &ask.responder {
                    self.write(answer_line(asked, &answer));
                }
            }
            Order::Rewind(request) => self.rewind(request),
            Order::Acp(_) => {}
        }
    }

    fn is_alive(&self) -> bool {
        !self.exited.load(Ordering::SeqCst)
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(child) = child.as_mut() {
            child.kill().unwrap_or_default();
        }
    }

    fn run(&self, command: &SlashCommand) {
        self.queue(Prompt::from(command.to_string()), None);
    }
}

struct RewindJob {
    session: Option<ResumeId>,
    request_id: String,
    outbox: Sender<Outbound>,
    awaited: AwaitedReply,
    host: krad_host::HostLink,
}

impl RewindJob {
    fn run(&self, request: &RewindRequest) -> Result<Rewound, RewindError> {
        let RewindPoint::Message(prompt) = &request.point else {
            return Err(RewindError::UnknownPoint);
        };
        let session = self.session.clone().ok_or(RewindError::NoSession)?;
        let home = request.home.as_deref().ok_or(RewindError::NoHome)?;
        let file = claude_conversation_file(&self.host, home, &session)
            .ok_or(RewindError::NoTranscript)?;
        let text = self
            .host
            .read_to_string(&file)
            .map_err(std::io::Error::from)?;
        let cut = transcript_cut(&text, prompt)?;
        if request.scope.restores_files() {
            self.restore_files(prompt)?;
        }
        Ok(Rewound::Restart(cut.session(session, prompt)))
    }

    fn restore_files(&self, prompt: &RewindId) -> Result<(), RewindError> {
        let (reply, replied) = channel();
        *self.awaited.lock().unwrap_or_else(PoisonError::into_inner) = Some(Awaited {
            request_id: self.request_id.clone(),
            reply,
        });
        self.outbox
            .send(Outbound::Line(rewind_files_line(&self.request_id, prompt)))
            .map_err(|_| RewindError::Stopped)?;
        replied
            .recv_timeout(FILES_REPLY_WAIT)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => RewindError::Timeout,
                RecvTimeoutError::Disconnected => RewindError::Stopped,
            })?
    }
}

pub(super) fn live_event(event: ChatEvent) -> LiveEvent {
    let ChatEvent::Error(error) = event else {
        return LiveEvent::Chat(event);
    };
    if error.contains(DROPS_TURN_REFUSED_TEXT) {
        return LiveEvent::Refused(ResumeRefused::TurnNotDropped(error));
    }
    if error.contains(MISSING_CONVERSATION_TEXT) {
        return LiveEvent::Refused(ResumeRefused::MissingConversation(error));
    }
    LiveEvent::Chat(ChatEvent::Error(error))
}

const fn known_session(session: &LiveSession) -> Option<&ResumeId> {
    match session {
        LiveSession::Start(id) | LiveSession::Resume(id) => Some(id),
        LiveSession::Fork(_) | LiveSession::Cut(_) => None,
    }
}

pub(super) fn spawn_claude(
    config: &LiveConfig,
    session: &LiveSession,
    mut sink: impl FnMut(LiveEvent) + Send + 'static,
) -> Result<ClaudeProcess, ChatError> {
    let args = live_chat_command(config, session);
    let mut env = config.bus_vars.0.clone();
    env.push((CHECKPOINTING_VAR.to_owned(), CHECKPOINTING_ON.to_owned()));
    let Harness {
        child,
        stdin,
        stdout,
        stderr,
    } = spawn_harness(&config.runner, &config.cwd, &args, &env)?;
    let (outbox, inbox) = channel();
    if let Some(stdin) = stdin {
        thread::spawn(move || write_messages(stdin, &inbox));
    }
    let (events, delivered) = channel();
    thread::spawn(move || {
        for event in delivered {
            sink(event);
        }
    });
    let stderr_tail = Arc::new(Mutex::new(String::new()));
    let stderr_reader = stderr.map(|stderr| {
        let tail = Arc::clone(&stderr_tail);
        thread::spawn(move || keep_stderr_tail(stderr, &tail))
    });
    let claude = ClaudeProcess {
        outbox,
        events: events.clone(),
        child: Arc::new(Mutex::new(Some(child))),
        pending: Arc::new(AtomicUsize::new(0)),
        stopped: Arc::new(AtomicBool::new(false)),
        exited: Arc::new(AtomicBool::new(false)),
        next_request: AtomicU64::new(0),
        session: Arc::new(Mutex::new(known_session(session).cloned())),
        awaited: Arc::default(),
        host: config.runner.link(),
    };
    let routing = Routing {
        init_id: claude.request_id(),
        pending: Arc::clone(&claude.pending),
        awaited: Arc::clone(&claude.awaited),
        session: Arc::clone(&claude.session),
        usage: UsageAsk::new(claude.outbox.clone(), Arc::clone(&config.usage_budget)),
    };
    claude.write(initialize_line(&routing.init_id));
    routing.usage.ask();
    let child = Arc::clone(&claude.child);
    let stopped = Arc::clone(&claude.stopped);
    let exited = Arc::clone(&claude.exited);
    thread::spawn(move || {
        let mut emit = |event| events.send(event).unwrap_or_default();
        if let Some(stdout) = stdout {
            for line in stdout {
                let Ok(line) = line else { break };
                routing.route_line(&line.text, line.origin, &mut emit);
            }
        }
        if let Some(reader) = stderr_reader {
            reader.join().unwrap_or_default();
        }
        let taken = child.lock().unwrap_or_else(PoisonError::into_inner).take();
        let status = taken.and_then(|mut child| child.wait().ok());
        exited.store(true, Ordering::SeqCst);
        routing.abandon_reply();
        let stderr = stderr_tail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(event) = closing_event(
            routing.pending.load(Ordering::SeqCst) == 0,
            stopped.load(Ordering::SeqCst),
            status.and_then(|status| status.code()),
            status.is_some_and(|status| status.success()),
            &stderr,
        ) {
            emit(live_event(event));
        }
    });
    Ok(claude)
}

fn write_messages(mut stdin: ChildStdin, inbox: &Receiver<Outbound>) {
    while let Ok(outbound) = inbox.recv() {
        let line = outbound.into_line();
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
    fn the_usage_ask_after_each_turn_keeps_the_shared_budget_gap() {
        let (outbox, sent) = channel();
        let usage = UsageAsk::new(outbox, Arc::default());
        usage.ask();
        usage.ask();
        usage.ask();
        let asked: Vec<Outbound> = sent.try_iter().collect();
        assert_eq!(asked.len(), 1);
        assert!(matches!(&asked[0], Outbound::Line(line) if line.contains(USAGE_REQUEST_PREFIX)));
    }

    #[test]
    fn two_chats_sharing_one_budget_ask_for_usage_once_within_the_gap() {
        let budget: Arc<UsageBudget> = Arc::default();
        let (first_outbox, first_sent) = channel();
        let (second_outbox, second_sent) = channel();
        let first = UsageAsk::new(first_outbox, Arc::clone(&budget));
        let second = UsageAsk::new(second_outbox, Arc::clone(&budget));
        first.ask();
        second.ask();
        assert_eq!(first_sent.try_iter().count(), 1);
        assert_eq!(second_sent.try_iter().count(), 0);
    }
}
