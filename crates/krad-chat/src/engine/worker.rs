mod answer;
mod batch;
mod compact;
mod rewind;
mod subagents;

use krad_domain::{
    ChatEvent, ContextWindow, Effort, GatewayAccess, PermissionMode, Prompt, RewindPoint,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;

use super::attachments::Attachments;
use super::mcp::McpToolbox;
use super::policy::Allowed;
use super::prompt::expand_skill_call;
use super::provider::stream;
use super::session::{Chosen, Record, SessionFile};
use super::snapshots::Snapshots;
use super::tools::{Background, Ran, Workspace};
use super::wire::{Block, Delta, Message, Protocol, Reply, Request, Stop, ToolSpec};
use crate::control::LiveEvent;
use crate::gateway::GatewayError;
use crate::gateway::Vendor;
use crate::live::Order;
use crate::mention::shown_path;
use crate::skills::SkillFile;
use batch::Batch;
use subagents::SubagentNews;

const MAX_OUTPUT_TOKENS: u64 = 32_768;
const NO_MODEL_TEXT: &str = "Pick a Gateway model to start the chat";

pub(crate) enum Inbound {
    Order(Order),
    Delta {
        attempt: u64,
        delta: Delta,
    },
    Replied {
        attempt: u64,
        reply: Result<Reply, GatewayError>,
    },
    Ran {
        attempt: u64,
        ran: Ran,
    },
    Compacted {
        attempt: u64,
        summary: Result<String, GatewayError>,
    },
    Subagent {
        call: String,
        news: SubagentNews,
    },
    Stop,
}

#[derive(Clone)]
pub(crate) struct Setup {
    pub(crate) access: GatewayAccess,
    pub(crate) workspace: Workspace,
    pub(crate) system: String,
    pub(crate) skills: Vec<SkillFile>,
    pub(crate) vendors: HashMap<String, Vendor>,
    pub(crate) windows: HashMap<String, ContextWindow>,
    pub(crate) tools: Vec<ToolSpec>,
    pub(crate) mcp: Arc<McpToolbox>,
}

impl Setup {
    pub(crate) fn vendor(&self, model: &str) -> Vendor {
        self.vendors.get(model).copied().unwrap_or_default()
    }

    pub(crate) fn window(&self, model: Option<&Chosen>) -> ContextWindow {
        model
            .and_then(|model| self.windows.get(&model.id))
            .copied()
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Chat,
    Subagent,
}

enum Phase {
    Idle,
    Streaming,
    Compacting,
    Tools(Box<Batch>),
}

pub(crate) struct Worker<S> {
    role: Role,
    setup: Setup,
    sink: S,
    session: SessionFile,
    inbound: Sender<Inbound>,
    messages: Vec<Message>,
    model: Option<Chosen>,
    effort: Option<Effort>,
    mode: PermissionMode,
    allowed: Allowed,
    background: Background,
    phase: Phase,
    queued: Vec<Message>,
    cancel: Arc<AtomicBool>,
    attempt: u64,
    thinking: String,
    context: u64,
    snapshots: Snapshots,
    prompts: usize,
    turn: Option<usize>,
}

pub(crate) struct Start {
    pub(crate) setup: Setup,
    pub(crate) session: SessionFile,
    pub(crate) messages: Vec<Message>,
    pub(crate) model: Option<Chosen>,
    pub(crate) effort: Option<Effort>,
    pub(crate) mode: PermissionMode,
    pub(crate) snapshots: Snapshots,
    pub(crate) prompts: usize,
    pub(crate) turn: Option<usize>,
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(crate) fn new(
        start: Start,
        sink: S,
        inbound: Sender<Inbound>,
        background: Background,
    ) -> Self {
        Self {
            role: Role::Chat,
            setup: start.setup,
            sink,
            session: start.session,
            inbound,
            messages: start.messages,
            model: start.model,
            effort: start.effort,
            mode: start.mode,
            allowed: Allowed::default(),
            background,
            phase: Phase::Idle,
            queued: Vec::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            attempt: 0,
            thinking: String::new(),
            context: 0,
            snapshots: start.snapshots,
            prompts: start.prompts,
            turn: start.turn,
        }
    }

    fn subagent(start: Start, sink: S, inbound: Sender<Inbound>, allowed: Allowed) -> Self {
        Self {
            role: Role::Subagent,
            allowed,
            ..Self::new(start, sink, inbound, Background::default())
        }
    }

    fn emit(&mut self, event: ChatEvent) {
        (self.sink)(LiveEvent::Chat(event));
    }

    fn record(&mut self, record: &Record) {
        if let Err(error) = self.session.append(record) {
            self.emit(ChatEvent::Error(error.to_string()));
        }
    }

    fn push_message(&mut self, message: Message) {
        self.record(&Record::Message(message.clone()));
        let prompted = matches!(message, Message::User { .. }) && self.role == Role::Chat;
        self.messages.push(message);
        if prompted {
            self.turn = Some(self.prompts);
            self.emit(ChatEvent::Prompted(RewindPoint::Prompt(self.prompts)));
            self.prompts += 1;
        }
    }

    pub(crate) fn run(&mut self, inbox: &Receiver<Inbound>) {
        for inbound in inbox {
            match inbound {
                Inbound::Order(order) => self.order(order),
                Inbound::Delta { attempt, delta } if attempt == self.attempt => self.delta(delta),
                Inbound::Replied { attempt, reply } if attempt == self.attempt => {
                    self.replied(reply);
                }
                Inbound::Ran { attempt, ran } if attempt == self.attempt => self.ran(ran),
                Inbound::Compacted { attempt, summary } if attempt == self.attempt => {
                    self.compacted(summary);
                }
                Inbound::Subagent { call, news } => self.subagent_news(call, news),
                Inbound::Stop => return self.background.stop_all(),
                Inbound::Delta { .. }
                | Inbound::Replied { .. }
                | Inbound::Ran { .. }
                | Inbound::Compacted { .. } => {}
            }
            if self.role == Role::Subagent && matches!(self.phase, Phase::Idle) {
                return self.background.stop_all();
            }
        }
    }

    fn order(&mut self, order: Order) {
        match order {
            Order::Send(prompt) => self.send(&prompt),
            Order::Interrupt => self.interrupt(),
            Order::Model(model) => {
                let id = model.as_str().to_owned();
                let chosen = Chosen {
                    vendor: self.setup.vendor(&id),
                    id,
                };
                self.record(&Record::from(&chosen));
                let window = self.setup.window(Some(&chosen));
                self.emit(ChatEvent::Model(chosen.id.clone()));
                self.emit(ChatEvent::ContextWindow(window.tokens()));
                self.model = Some(chosen);
            }
            Order::Effort(effort) => self.effort = Some(effort),
            Order::Mode(mode) => {
                self.mode = mode;
                self.tell_subagents(mode);
                self.emit(ChatEvent::PermissionMode(mode));
            }
            Order::Answer(ask, answer) => self.answer(&ask, &answer),
            Order::Rewind(request) => self.rewind(&request),
        }
    }

    fn send(&mut self, prompt: &Prompt) {
        let text = prompt.text_with_paths(shown_path);
        let text = expand_skill_call(&text, &self.setup.skills).unwrap_or(text);
        let images = prompt.images().cloned().collect();
        let message = Message::User { text, images };
        if !matches!(self.phase, Phase::Idle) {
            self.queued.push(message);
            return;
        }
        self.push_message(message);
        self.begin_request();
    }

    fn deliver_queued(&mut self) {
        for message in std::mem::take(&mut self.queued) {
            self.push_message(message);
        }
    }

    fn next_attempt(&mut self) -> (u64, Arc<AtomicBool>) {
        self.attempt += 1;
        if self.cancel.load(Ordering::SeqCst) {
            self.cancel = Arc::new(AtomicBool::new(false));
        }
        (self.attempt, Arc::clone(&self.cancel))
    }

    fn begin_request(&mut self) {
        let Some(model) = self.model.clone() else {
            self.phase = Phase::Idle;
            return self.emit(ChatEvent::Error(NO_MODEL_TEXT.to_owned()));
        };
        if self.compact_if_needed(&model) {
            return;
        }
        let (attempt, cancel) = self.next_attempt();
        let access = self.setup.access.clone();
        let protocol = Protocol::from(model.vendor);
        let system = self.setup.system.clone();
        let messages = self.messages.clone();
        let tools = self.setup.tools.clone();
        let effort = self.effort;
        let inbound = self.inbound.clone();
        self.phase = Phase::Streaming;
        thread::spawn(move || {
            let images = Attachments::read(&messages);
            let request = Request {
                model: &model.id,
                system: &system,
                messages: &messages,
                images: &images,
                tools: &tools,
                native: None,
                effort,
                max_tokens: MAX_OUTPUT_TOKENS,
            };
            let deltas = inbound.clone();
            let reply = stream(&access, protocol, &request, &cancel, &mut |delta| {
                deltas
                    .send(Inbound::Delta { attempt, delta })
                    .unwrap_or_default();
            });
            inbound
                .send(Inbound::Replied { attempt, reply })
                .unwrap_or_default();
        });
    }

    fn flush_thinking(&mut self) {
        if self.thinking.trim().is_empty() {
            self.thinking.clear();
            return;
        }
        let thought = std::mem::take(&mut self.thinking);
        self.emit(ChatEvent::Thinking(thought));
    }

    fn delta(&mut self, delta: Delta) {
        match delta {
            Delta::Thinking(text) => self.thinking.push_str(&text),
            Delta::Text(text) => {
                self.flush_thinking();
                self.emit(ChatEvent::TextDelta(text));
            }
        }
    }

    fn replied(&mut self, reply: Result<Reply, GatewayError>) {
        self.flush_thinking();
        let reply = match reply {
            Ok(reply) => reply,
            Err(GatewayError::Cancelled) => return self.finish_turn(),
            Err(error) => {
                self.phase = Phase::Idle;
                self.queued.clear();
                return self.emit(ChatEvent::Error(error.to_string()));
            }
        };
        self.context = reply.usage.context();
        self.emit(ChatEvent::ContextTokens(self.context));
        let calls: Vec<_> = reply
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Call(call) => Some(call.clone()),
                Block::Text { .. } | Block::Thinking { .. } => None,
            })
            .collect();
        self.push_message(Message::Assistant {
            blocks: reply.blocks,
            usage: reply.usage,
            stop: reply.stop,
        });
        if calls.is_empty() {
            if self.queued.is_empty() {
                return self.end_answer(reply.stop);
            }
            if let Some(problem) = reply.stop.problem() {
                (self.sink)(LiveEvent::Notice(problem.to_owned()));
            }
            self.deliver_queued();
            return self.begin_request();
        }
        self.phase = Phase::Tools(Box::new(Batch::new(calls)));
        self.start_subagents();
        self.advance_batch();
    }

    fn finish_turn(&mut self) {
        self.phase = Phase::Idle;
        self.emit(ChatEvent::Done);
    }

    fn end_answer(&mut self, stop: Stop) {
        let Some(problem) = stop.problem() else {
            return self.finish_turn();
        };
        self.phase = Phase::Idle;
        self.emit(ChatEvent::Error(problem.to_owned()));
    }

    fn interrupt(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.attempt += 1;
        self.thinking.clear();
        self.queued.clear();
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Idle => {}
            Phase::Streaming | Phase::Compacting => self.finish_turn(),
            Phase::Tools(batch) => {
                let outcomes = batch.interrupted();
                self.push_message(Message::Results { outcomes });
                self.finish_turn();
            }
        }
    }
}
