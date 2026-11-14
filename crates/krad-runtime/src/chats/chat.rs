use krad_chat::{new_session_id, LiveChat, LiveSession, ResumeRefused, SlashCommand, ToolAsk};
use krad_domain::{
    fold_entry, fold_prompt, AgentPreset, AskVerdict, ChatEntry, ChatLogChange, ChatMessage,
    ChatOrigin, CommandOffer, GitObjectId, Mention, Prompt, ResumeId,
};
use krad_protocol::{AgentRef, ChatAskId, ChatContext, ChatState, EventKind};
use krad_store::AcpTranscript;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::SystemTime;

use super::acp::AcpChat;
use super::backend::{ChatLaunch, Choices};
use super::brief::ChatBrief;
use super::cloud::CloudChat;
use super::mcp_prep::McpPrep;
use super::ChatPlace;
use crate::launches::AgentRole;

mod log;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Batch {
    pub(super) commands: Vec<SlashCommand>,
    pub(super) message: Option<Prompt>,
}

impl Batch {
    fn results(&self) -> usize {
        self.commands.len() + usize::from(self.message.is_some())
    }
}

pub(super) struct Rewinding {
    pub(super) index: usize,
    pub(super) prompt: Prompt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Earlier {
    pub(super) file: PathBuf,
    pub(super) end: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum Shown {
    #[default]
    Whole,
    FromTail(Earlier),
    ReadingEarlier(Earlier),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum HistoryWait {
    #[default]
    No,
    ForServer,
    ForRunner,
}

pub(super) struct Chat {
    pub(super) id: AgentRef,
    pub(super) place: ChatPlace,
    pub(super) harness: AgentPreset,
    pub(super) prepared: Option<ChatLaunch>,
    pub(super) brief: ChatBrief,
    pub(super) mcp: McpPrep,
    pub(super) started_as: Option<AgentRole>,
    pub(super) choices: Choices,
    pub(super) session: LiveSession,
    pub(super) live: Option<LiveChat>,
    pub(super) generation: u64,
    pub(super) opened: u64,
    pub(super) home: Option<PathBuf>,
    pub(super) confirmed: bool,
    pub(super) fell_back: bool,
    pub(super) messages: Vec<ChatMessage>,
    pub(super) outbox: VecDeque<Prompt>,
    pub(super) commands: Vec<SlashCommand>,
    pub(super) last_batch: Option<Batch>,
    pub(super) outstanding: usize,
    pub(super) working: bool,
    pub(super) working_since: Option<SystemTime>,
    pub(super) asks: BTreeMap<ChatAskId, ToolAsk>,
    pub(super) rewinding: Option<Rewinding>,
    pub(super) resume: Option<ResumeId>,
    pub(super) model: Option<String>,
    pub(super) context: ChatContext,
    pub(super) problem: Option<String>,
    pub(super) notices: Vec<String>,
    pub(super) start_notices: Vec<String>,
    pub(super) shown: Shown,
    pub(super) origin: Option<ChatOrigin>,
    pub(super) checkpoints: VecDeque<Option<GitObjectId>>,
    pub(super) checkpointing: Option<Batch>,
    pub(super) history: HistoryWait,
    pub(super) acp: Option<AcpChat>,
    pub(super) transcript: Option<AcpTranscript>,
    pub(super) offered: Vec<CommandOffer>,
    pub(super) account: Option<String>,
    pub(super) cloud: Option<CloudChat>,
    told: Option<ChatState>,
}

impl Chat {
    pub(super) fn new(id: AgentRef, start: super::ChatStart, place: ChatPlace) -> Self {
        let resume = start.session.history().cloned();
        let prepared = start.prepared.map(|prepared| prepared.launch);
        Self {
            id,
            place,
            harness: start.harness,
            choices: prepared
                .as_ref()
                .map_or_else(|| Choices::from(&start.brief), Choices::from),
            acp: prepared.as_ref().map_or_else(
                || {
                    start
                        .harness
                        .acp_agent()
                        .map(|_| AcpChat::wanting(start.brief.acp.clone()))
                },
                AcpChat::of,
            ),
            prepared,
            brief: start.brief,
            mcp: McpPrep::Unread,
            started_as: None,
            session: start.session,
            live: None,
            generation: 0,
            opened: 0,
            home: None,
            confirmed: false,
            fell_back: false,
            messages: start.messages,
            outbox: start.queued.into(),
            commands: Vec::new(),
            last_batch: None,
            outstanding: 0,
            working: false,
            working_since: None,
            asks: BTreeMap::new(),
            rewinding: None,
            resume,
            model: None,
            context: ChatContext::default(),
            problem: None,
            notices: Vec::new(),
            start_notices: start.notices,
            shown: Shown::Whole,
            origin: None,
            checkpoints: VecDeque::new(),
            checkpointing: None,
            history: HistoryWait::No,
            transcript: None,
            offered: Vec::new(),
            account: None,
            cloud: None,
            told: None,
        }
    }

    pub(super) fn state(&self) -> ChatState {
        ChatState {
            harness: self.harness,
            working: self.working,
            alive: self.live.as_ref().is_some_and(LiveChat::is_alive),
            resume: self.resume.clone(),
            model: self.model.clone(),
            chosen_model: self.choices.model.clone(),
            effort: self.choices.effort,
            permission_mode: self.choices.permission_mode,
            context: self.context.clone(),
            queued: self.outbox.iter().cloned().collect(),
            problem: self.problem.clone(),
            notices: self.notices.clone(),
            working_since: self.working_since,
            earlier: self.shown != Shown::Whole,
            origin: self.origin,
            commands: self.offered.clone(),
            account: self.account.clone(),
            acp: self.acp.as_ref().map(|acp| acp.shown.clone()),
            mcp_trust: self.mcp.asked(),
            launch: self.shown_launch(),
        }
    }

    pub(super) fn tell_state(&mut self, outbox: &mut Vec<EventKind>) {
        let state = self.state();
        if self.told.as_ref() == Some(&state) {
            return;
        }
        self.told = Some(state.clone());
        outbox.push(EventKind::ChatState {
            chat: self.id.clone(),
            state: Box::new(state),
        });
    }

    pub(super) fn live(&self) -> Option<&LiveChat> {
        self.live.as_ref().filter(|live| live.is_alive())
    }

    pub(super) fn fold(&mut self, entry: &ChatEntry, outbox: &mut Vec<EventKind>) {
        if let Some(transcript) = &self.transcript {
            transcript.keep(entry);
        }
        let before = self.messages.len();
        let touched = fold_entry(&mut self.messages, entry);
        let changes = ChatLogChange::after(entry, &self.messages, before, touched);
        self.tell_changes(changes, outbox);
    }

    pub(super) fn fold_sent(&mut self, prompt: &Prompt, outbox: &mut Vec<EventKind>) {
        if let Some(transcript) = &self.transcript {
            transcript.keep(&self.user_entry(prompt));
        }
        let before = self.messages.len();
        let touched = fold_prompt(&mut self.messages, prompt, self.harness);
        self.tell_messages(before, touched, outbox);
        self.problem = None;
        self.notices = std::mem::take(&mut self.start_notices);
    }

    fn user_entry(&self, prompt: &Prompt) -> ChatEntry {
        ChatEntry::User {
            text: prompt.text.clone(),
            preset: self.harness,
            model: self.choices.model.clone(),
            images: prompt
                .mentions
                .iter()
                .filter_map(|mention| match mention {
                    Mention::Image(image) => Some(image.clone()),
                    Mention::Path(_) => None,
                })
                .collect(),
        }
    }

    pub(super) fn begin_work(&mut self) {
        if !self.working {
            self.working_since = Some(SystemTime::now());
        }
        self.working = true;
    }

    pub(super) fn end_work(&mut self) {
        self.working = false;
        self.working_since = None;
    }

    pub(super) fn is_free(&self) -> bool {
        !self.working
            && self.asks.is_empty()
            && self.rewinding.is_none()
            && self.checkpointing.is_none()
    }

    pub(super) fn has_work(&self) -> bool {
        self.is_free() && (!self.commands.is_empty() || !self.outbox.is_empty())
    }

    pub(super) fn take_batch(&mut self) -> Batch {
        Batch {
            commands: std::mem::take(&mut self.commands),
            message: self.outbox.pop_front(),
        }
    }

    pub(super) fn handed_over(&mut self, batch: Batch) {
        self.outstanding += batch.results();
        if batch.message.is_some() {
            self.begin_work();
        }
        self.last_batch = Some(batch);
    }

    pub(super) fn session_reported(&mut self, resume: ResumeId) {
        self.confirmed = true;
        self.resume = Some(resume.clone());
        self.session = LiveSession::Resume(resume);
    }

    pub(super) fn opens_turn(&mut self) -> bool {
        if self.working || self.outstanding > 0 {
            return false;
        }
        self.outstanding = 1;
        self.begin_work();
        true
    }

    pub(super) fn closes_turn(&mut self, ended: bool) -> bool {
        self.outstanding = self.outstanding.saturating_sub(1);
        if ended {
            self.outstanding = 0;
        }
        if self.outstanding > 0 {
            return false;
        }
        self.end_work();
        true
    }

    pub(super) fn retries(&mut self, refused: &ResumeRefused) -> Option<Batch> {
        match refused {
            ResumeRefused::MissingConversation(_) => {
                let batch = self.falls_back()?;
                if self.acp.is_some() {
                    self.notices.push(refused.to_string());
                }
                Some(batch)
            }
            ResumeRefused::TurnNotDropped(_) => self.drops_turn_again(),
        }
    }

    fn falls_back(&mut self) -> Option<Batch> {
        let reported = self.confirmed || self.live().is_some();
        if self.fell_back || reported || self.session.history().is_none() {
            return None;
        }
        self.fell_back = true;
        self.session = LiveSession::Start(new_session_id());
        self.live = None;
        self.outstanding = 0;
        Some(self.last_batch.take().unwrap_or_default())
    }

    fn drops_turn_again(&mut self) -> Option<Batch> {
        let LiveSession::Cut(cut) = &mut self.session else {
            return None;
        };
        if self.confirmed {
            return None;
        }
        cut.drops.take()?;
        self.live = None;
        self.outstanding = 0;
        Some(self.last_batch.take().unwrap_or_default())
    }

    pub(super) fn cancel_asks(&mut self, outbox: &mut Vec<EventKind>) {
        for (ask, held) in std::mem::take(&mut self.asks) {
            self.fold(
                &ChatEntry::Asked(held.kind.outcome(AskVerdict::Cancelled)),
                outbox,
            );
            outbox.push(EventKind::ChatAskSettled {
                chat: self.id.clone(),
                ask,
            });
        }
    }
}
