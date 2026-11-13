mod answers;
mod chat;
mod heard;
mod history;
mod queue;

use krad_chat::{start_chat, ChatBackend, LiveConfig, LiveSession};
use krad_domain::{AgentPreset, AttentionKind, ChatMessage, Host, ModelOffer, Prompt, UsageReport};
use krad_host::HostLink;
use krad_protocol::{
    AgentRef, ChatAsk, ChatCommand, ChatPage, ChatSummary, ErrorCode, EventKind, Reply, UsageNews,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender};

use crate::input::Input;
use crate::jobs::Jobs;
use crate::runtime::refused;
use chat::Chat;
pub(crate) use history::ChatOpening;
use queue::{ChatQueue, Heard};

const CHAT_PAGE: usize = 64;
const UNKNOWN_CHAT: &str = "no chat with this id";

#[derive(Clone, Debug)]
pub struct ChatPlace {
    pub host: Host,
    pub link: HostLink,
    pub root: Option<PathBuf>,
}

#[derive(Debug)]
pub struct ChatStart {
    pub harness: AgentPreset,
    pub config: LiveConfig,
    pub session: LiveSession,
    pub place: ChatPlace,
    pub messages: Vec<ChatMessage>,
    pub queued: Vec<Prompt>,
}

#[must_use]
pub const fn runs_in_runtime(harness: AgentPreset) -> bool {
    matches!(harness, AgentPreset::ClaudeCode)
}

#[derive(Debug)]
pub(crate) enum ChatEffect {
    Raise {
        chat: AgentRef,
        kind: AttentionKind,
    },
    Usage {
        host: Host,
        harness: AgentPreset,
        news: UsageNews,
    },
    Models {
        host: Host,
        harness: AgentPreset,
        models: Vec<ModelOffer>,
    },
}

pub(crate) struct Chats {
    open: HashMap<AgentRef, Chat>,
    queue: ChatQueue,
    heard: Receiver<Heard>,
    next_ask: u64,
    next_generation: u64,
}

impl std::fmt::Debug for Chats {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Chats")
            .field("open", &self.open.len())
            .finish_non_exhaustive()
    }
}

pub(crate) struct ChatWork<'a> {
    pub(crate) jobs: &'a Jobs,
    pub(crate) outbox: &'a mut Vec<EventKind>,
    pub(crate) effects: &'a mut Vec<ChatEffect>,
}

impl Chats {
    pub(crate) fn new(wake: SyncSender<Input>) -> Self {
        let (queue, heard) = ChatQueue::open(wake);
        Self {
            open: HashMap::new(),
            queue,
            heard,
            next_ask: 0,
            next_generation: 0,
        }
    }

    fn next_generation(&mut self) -> u64 {
        self.next_generation += 1;
        self.next_generation
    }

    #[tracing::instrument(skip_all, fields(environment = id.environment.as_str(), chat = id.terminal.0))]
    pub(crate) fn open(&mut self, id: &AgentRef, start: ChatStart, work: &mut ChatWork<'_>) {
        if !runs_in_runtime(start.harness) {
            tracing::warn!("a chat the runtime cannot run was handed to it");
            return;
        }
        let mut chat = Chat::new(id.clone(), start);
        chat.opened = self.next_generation();
        work.outbox.push(EventKind::ChatOpened {
            chat: id.clone(),
            state: Box::new(chat.state()),
        });
        history::read(&chat, work.jobs);
        tracing::info!("chat opened in the runtime");
        self.open.insert(id.clone(), chat);
        self.flush(id, work.outbox);
    }

    pub(crate) fn close(&mut self, id: &AgentRef, outbox: &mut Vec<EventKind>) {
        let Some(chat) = self.open.remove(id) else {
            return;
        };
        if let Some(live) = &chat.live {
            live.stop();
        }
        outbox.push(EventKind::ChatClosed { chat: id.clone() });
    }

    pub(crate) fn close_environment(&mut self, environment: &str, outbox: &mut Vec<EventKind>) {
        let closing: Vec<AgentRef> = self
            .open
            .keys()
            .filter(|id| id.environment.as_str() == environment)
            .cloned()
            .collect();
        for id in closing {
            self.close(&id, outbox);
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: ChatCommand,
        work: &mut ChatWork<'_>,
    ) -> Option<Reply> {
        let id = command.chat().cloned()?;
        if !self.open.contains_key(&id) {
            return Some(refused(ErrorCode::NotFound, UNKNOWN_CHAT));
        }
        let reply = match command {
            ChatCommand::Send { prompt, .. } | ChatCommand::Queue { prompt, .. } => {
                self.queue_prompt(&id, prompt)
            }
            ChatCommand::Interrupt { .. } => self.interrupt(&id),
            ChatCommand::Rewind {
                index,
                prompt,
                scope,
                ..
            } => self.rewind(&id, (index, prompt), scope, work.outbox),
            ChatCommand::SetModel { model, .. } => self.set_model(&id, model),
            ChatCommand::SetEffort { effort, .. } => self.set_effort(&id, effort),
            ChatCommand::SetPermissionMode { mode, .. } => self.set_permission_mode(&id, mode),
            ChatCommand::SetThinking { thinking, .. } => self.set_thinking(&id, thinking),
            ChatCommand::AnswerAsk { ask, answer, .. } => self.answer(&id, ask, &answer, work),
            ChatCommand::AnswerQuestion { ask, answers, .. } => {
                self.answer(&id, ask, &krad_domain::AskAnswer::Answers(answers), work)
            }
            ChatCommand::ApprovePlan { ask, mode, .. } => self.answer(
                &id,
                ask,
                &krad_domain::AskAnswer::ApprovePlan { mode },
                work,
            ),
            ChatCommand::Unknown => return None,
        };
        self.flush(&id, work.outbox);
        Some(reply)
    }

    pub(crate) fn flush(&mut self, id: &AgentRef, outbox: &mut Vec<EventKind>) {
        let generation = self.next_generation + 1;
        let Some(chat) = self.open.get_mut(id) else {
            return;
        };
        if chat.has_work() && ensure_live(chat, &self.queue, generation) {
            if chat.generation == generation {
                self.next_generation = generation;
            }
            let batch = chat.take_batch();
            if let Some(prompt) = &batch.message {
                chat.fold_sent(prompt, outbox);
                chat.problem = None;
            }
            hand_over(chat, batch);
        }
        chat.tell_state(outbox);
    }

    pub(crate) fn snapshot(&self) -> Vec<ChatSummary> {
        let mut summaries: Vec<ChatSummary> = self
            .open
            .values()
            .map(|chat| ChatSummary {
                state: chat.state(),
                tail: page(chat, chat.messages.len(), CHAT_PAGE),
                asks: asks_of(chat),
            })
            .collect();
        summaries.sort_by(|left, right| {
            let key = |summary: &ChatSummary| {
                (
                    summary.tail.chat.environment.as_str().to_owned(),
                    summary.tail.chat.terminal.0,
                )
            };
            key(left).cmp(&key(right))
        });
        summaries
    }

    pub(crate) fn page(&self, id: &AgentRef, before: usize, count: usize) -> Option<ChatPage> {
        self.open.get(id).map(|chat| page(chat, before, count))
    }

    fn spent(chat: &Chat, report: UsageReport) -> EventKind {
        let model = chat.model.as_deref().and_then(krad_domain::ModelId::parse);
        EventKind::ChatSpent {
            chat: chat.id.clone(),
            report: report.or_model(model),
        }
    }
}

fn page(chat: &Chat, before: usize, count: usize) -> ChatPage {
    let end = before.min(chat.messages.len());
    let from = end.saturating_sub(count);
    ChatPage {
        chat: chat.id.clone(),
        len: chat.messages.len(),
        from,
        messages: chat.messages[from..end].to_vec(),
    }
}

fn asks_of(chat: &Chat) -> Vec<ChatAsk> {
    chat.asks
        .iter()
        .map(|(ask, held)| ChatAsk {
            chat: chat.id.clone(),
            ask: *ask,
            kind: held.kind.clone(),
            can_allow_always: held.can_allow_always(),
        })
        .collect()
}

fn ensure_live(chat: &mut Chat, queue: &ChatQueue, generation: u64) -> bool {
    if chat.live().is_some() {
        return true;
    }
    let sink = queue.sink(chat.id.clone(), generation);
    match start_chat(
        ChatBackend::Claude(chat.config.clone()),
        &chat.session,
        sink,
    ) {
        Ok(live) => {
            chat.live = Some(live);
            chat.generation = generation;
            chat.confirmed = false;
            true
        }
        Err(error) => {
            tracing::debug!(%error, "a chat could not start");
            chat.problem = Some(error.to_string());
            false
        }
    }
}

fn hand_over(chat: &mut Chat, batch: chat::Batch) {
    let Some(live) = chat.live() else {
        return;
    };
    for command in &batch.commands {
        live.run(command);
    }
    if let Some(prompt) = &batch.message {
        live.send(prompt.clone());
    }
    chat.handed_over(batch);
}
