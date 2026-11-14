mod acp;
mod answers;
mod backend;
mod brief;
mod buffers;
mod chat;
mod checkpoints;
mod cloud;
mod digest_source;
mod heard;
mod history;
mod mcp_prep;
mod mentions;
mod queue;
mod resolve;
mod subagents_read;
mod summary;
mod transcript_tail;
mod workers;

use krad_chat::LiveSession;
use krad_domain::{
    AcpAuthMethodId, AgentPreset, AttentionKind, ChatMessage, CloudPlacement, CloudRunnerId, Host,
    McpProjectTrust, ModelOffer, Prompt, UsageReport,
};
use krad_host::HostLink;
use krad_protocol::{AgentRef, ChatCommand, ErrorCode, EventKind, Reply, UsageNews};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender};

use crate::adapters::AcpAdapters;
use crate::cloud_runs::CloudRuns;
use crate::gateway::GatewayReached;
use crate::input::Input;
use crate::jobs::Jobs;
use crate::opencode::OpencodeServers;
use crate::runtime::refused;
use acp::AcpChoice;
pub(crate) use acp::AcpSecretsRead;
use backend::ensure_live;
pub(crate) use backend::resolve_once;
pub use backend::{runs_in_runtime, ChatLaunch};
pub use brief::{ChatBrief, ChatRole, PreparedLaunch};
pub(crate) use buffers::{EditorBuffers, OfferedBuffers};
use chat::{Chat, HistoryWait};
pub(crate) use checkpoints::ChatCheckpoint;
pub(crate) use digest_source::DigestSource;
pub(crate) use history::{ChatEarlier, ChatOpening};
use krad_store::{acp_transcript_file, AcpTranscripts};
pub(crate) use mcp_prep::{McpLaunched, McpRead};
use mentions::MentionIndexes;
pub(crate) use mentions::{FilesIndexed, MentionQuery};
use queue::{ChatQueue, Heard};
pub(crate) use resolve::LaunchFacts;
pub(crate) use subagents_read::read_subagents;
use summary::asks_of;
pub(crate) use transcript_tail::read_tail;
pub(crate) use workers::Launch;

const CHAT_PAGE: usize = 64;
pub(crate) const UNKNOWN_CHAT: &str = "no chat with this id";
const NOT_ACP: &str = "only an ACP agent's chat takes this command";
const NOT_STARTED: &str = "the agent has not started yet";

#[derive(Clone, Debug)]
pub struct ChatPlace {
    pub host: Host,
    pub link: HostLink,
    pub root: Option<PathBuf>,
    pub cloud: Option<CloudPlacement>,
}

pub struct ChatStart {
    pub harness: AgentPreset,
    pub brief: ChatBrief,
    pub prepared: Option<PreparedLaunch>,
    pub session: LiveSession,
    pub messages: Vec<ChatMessage>,
    pub queued: Vec<Prompt>,
    pub notices: Vec<String>,
}

impl std::fmt::Debug for ChatStart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChatStart")
            .field("harness", &self.harness)
            .field("brief", &self.brief)
            .finish_non_exhaustive()
    }
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
    McpTrust {
        folder: PathBuf,
        trust: McpProjectTrust,
    },
}

pub(crate) struct Chats {
    open: HashMap<AgentRef, Chat>,
    transcripts: Option<AcpTranscripts>,
    queue: ChatQueue,
    heard: Receiver<Heard>,
    next_ask: u64,
    next_generation: u64,
    ended: Vec<DigestSource>,
    pub(crate) mentions: MentionIndexes,
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
    pub(crate) cloud: &'a mut CloudRuns,
    pub(crate) adapters: &'a mut AcpAdapters,
    pub(crate) gateway: Option<GatewayReached<'a>>,
    pub(crate) opencode: &'a mut OpencodeServers,
    pub(crate) outbox: &'a mut Vec<EventKind>,
    pub(crate) effects: &'a mut Vec<ChatEffect>,
    pub(crate) facts: LaunchFacts<'a>,
}

impl Chats {
    pub(crate) fn new(wake: SyncSender<Input>) -> Self {
        let (queue, heard) = ChatQueue::open(wake);
        Self {
            open: HashMap::new(),
            transcripts: AcpTranscripts::spawn(),
            queue,
            heard,
            next_ask: 0,
            next_generation: 0,
            ended: Vec::new(),
            mentions: MentionIndexes::default(),
        }
    }

    fn next_generation(&mut self) -> u64 {
        self.next_generation += 1;
        self.next_generation
    }

    #[tracing::instrument(skip_all, fields(environment = id.environment.as_str(), chat = id.terminal.0))]
    pub(crate) fn open(&mut self, id: &AgentRef, start: ChatStart, work: &mut ChatWork<'_>) {
        if self.rejoin(id, work.outbox) {
            return;
        }
        if !runs_in_runtime(start.harness) {
            tracing::warn!("a chat the runtime cannot run was handed to it");
            return;
        }
        let place = match &start.prepared {
            Some(prepared) => Some(prepared.place.clone()),
            None => work.facts.place(id),
        };
        let Some(place) = place else {
            tracing::warn!("a chat of an environment the runtime does not know was handed to it");
            return;
        };
        let mut chat = Chat::new(id.clone(), start, place);
        chat.opened = self.next_generation();
        if chat.acp.is_some() {
            chat.transcript = acp_transcript_file(&id.environment, id.terminal)
                .and_then(|file| self.transcripts.as_ref()?.transcript(file));
        }
        work.outbox.push(EventKind::ChatOpened {
            chat: id.clone(),
            state: Box::new(chat.state()),
        });
        history::read(&mut chat, work);
        tracing::info!("chat opened in the runtime");
        self.open.insert(id.clone(), chat);
        self.flush(id, work);
    }

    pub(crate) fn messages(&self, id: &AgentRef) -> Option<Vec<ChatMessage>> {
        self.open.get(id).map(|chat| chat.messages.clone())
    }

    pub(crate) fn close(&mut self, id: &AgentRef, outbox: &mut Vec<EventKind>) {
        let Some(chat) = self.open.remove(id) else {
            return;
        };
        if let Some(live) = &chat.live {
            live.stop();
        }
        self.ended.push(DigestSource::from(&chat));
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
            ChatCommand::Unqueue { index, .. } => self.unqueue(&id, index),
            ChatCommand::ShowEarlier { .. } => self.show_earlier(&id, work.jobs),
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
            ChatCommand::SetAcpMode { mode, .. } => self.choose_acp(&id, AcpChoice::Mode(mode)),
            ChatCommand::SetAcpOption { option, value, .. } => {
                self.choose_acp(&id, AcpChoice::Option(option, value))
            }
            ChatCommand::SignIn { method, .. } => self.sign_in(&id, method),
            ChatCommand::TrustMcp { trust, .. } => self.trust_mcp(&id, trust, work),
            ChatCommand::Open { .. }
            | ChatCommand::Close { .. }
            | ChatCommand::Page { .. }
            | ChatCommand::Mentions { .. }
            | ChatCommand::ForgetMentions
            | ChatCommand::Transcript { .. }
            | ChatCommand::Subagents { .. }
            | ChatCommand::History { .. }
            | ChatCommand::AttachImage { .. }
            | ChatCommand::ForgetImages { .. }
            | ChatCommand::ReadImage { .. }
            | ChatCommand::Unknown => return None,
        };
        self.flush(&id, work);
        Some(reply)
    }

    pub(crate) fn acp_secrets_read(&mut self, read: AcpSecretsRead, work: &mut ChatWork<'_>) {
        let Some(acp) = self
            .open
            .get_mut(&read.chat)
            .filter(|chat| chat.opened == read.opened)
            .and_then(|chat| chat.acp.as_mut())
        else {
            return;
        };
        acp.secrets_read(read.read);
        self.flush(&read.chat, work);
    }

    fn choose_acp(&mut self, id: &AgentRef, choice: AcpChoice) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        let Some(acp) = chat.acp.as_mut() else {
            return refused(ErrorCode::Unsupported, NOT_ACP);
        };
        acp.chose(&choice);
        if let Some(live) = chat.live.as_ref().filter(|live| live.is_alive()) {
            acp::tell(live, choice);
        }
        Reply::Accepted
    }

    fn sign_in(&mut self, id: &AgentRef, method: AcpAuthMethodId) -> Reply {
        let Some(chat) = self.open.get(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        if chat.acp.is_none() {
            return refused(ErrorCode::Unsupported, NOT_ACP);
        }
        let Some(live) = chat.live() else {
            return refused(ErrorCode::NotFound, NOT_STARTED);
        };
        live.sign_in(method);
        Reply::Accepted
    }

    pub(crate) fn opencode_ready(&mut self, host: &Host, work: &mut ChatWork<'_>) {
        for chat in self.open.values_mut() {
            if chat.history == HistoryWait::ForServer && chat.place.host == *host {
                chat.history = HistoryWait::No;
                history::read(chat, work);
            }
        }
        self.flush_all(work);
    }

    pub(crate) fn cloud_ready(&mut self, runner: CloudRunnerId, work: &mut ChatWork<'_>) {
        for chat in self.open.values_mut() {
            let on_runner = chat
                .place
                .cloud
                .as_ref()
                .is_some_and(|placement| placement.runner == runner);
            if on_runner && chat.history != HistoryWait::No {
                chat.history = HistoryWait::No;
                history::read(chat, work);
            }
        }
        self.flush_all(work);
    }

    pub(crate) fn runner_problem(
        &mut self,
        runner: CloudRunnerId,
        problem: &str,
        outbox: &mut Vec<EventKind>,
    ) {
        for chat in self.open.values_mut().filter(|chat| {
            chat.place
                .cloud
                .as_ref()
                .is_some_and(|placement| placement.runner == runner)
        }) {
            chat.problem = Some(problem.to_owned());
            chat.tell_state(outbox);
        }
    }

    pub(crate) fn flush_all(&mut self, work: &mut ChatWork<'_>) {
        let open: Vec<AgentRef> = self.open.keys().cloned().collect();
        for id in open {
            self.flush(&id, work);
        }
    }

    pub(crate) fn flush(&mut self, id: &AgentRef, work: &mut ChatWork<'_>) {
        let generation = self.next_generation + 1;
        let Some(chat) = self.open.get_mut(id) else {
            return;
        };
        if chat.has_work() && ensure_live(chat, &self.queue, generation, work) {
            if chat.generation == generation {
                self.next_generation = generation;
            }
            let batch = chat.take_batch();
            if let Some(prompt) = &batch.message {
                chat.fold_sent(prompt, work.outbox);
            }
            if !checkpoints::take_first(chat, batch.clone(), work.jobs) {
                hand_over(chat, batch);
            }
        }
        chat.tell_state(work.outbox);
    }

    fn spent(chat: &Chat, report: UsageReport) -> EventKind {
        let model = chat.model.as_deref().and_then(krad_domain::ModelId::parse);
        EventKind::ChatSpent {
            chat: chat.id.clone(),
            report: report.or_model(model),
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
