use krad_chat::{
    new_session_id, LiveChat, LiveConfig, LiveSession, ResumeRefused, SlashCommand, ToolAsk,
};
use krad_domain::{
    fold_entry, fold_prompt, AgentPreset, AskVerdict, ChatEntry, ChatLogChange, ChatMessage,
    Prompt, ResumeId,
};
use krad_protocol::{AgentRef, ChatAskId, ChatContext, ChatState, EventKind};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

use super::ChatPlace;

const NOTICE_SEPARATOR: &str = "\n";

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

pub(super) struct Chat {
    pub(super) id: AgentRef,
    pub(super) place: ChatPlace,
    pub(super) harness: AgentPreset,
    pub(super) config: LiveConfig,
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
    pub(super) asks: BTreeMap<ChatAskId, ToolAsk>,
    pub(super) rewinding: Option<Rewinding>,
    pub(super) resume: Option<ResumeId>,
    pub(super) model: Option<String>,
    pub(super) context: ChatContext,
    pub(super) problem: Option<String>,
    told: Option<ChatState>,
}

impl Chat {
    pub(super) fn new(id: AgentRef, start: super::ChatStart) -> Self {
        let resume = start.session.history().cloned();
        Self {
            id,
            place: start.place,
            harness: start.harness,
            config: start.config,
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
            asks: BTreeMap::new(),
            rewinding: None,
            resume,
            model: None,
            context: ChatContext::default(),
            problem: None,
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
            chosen_model: self.config.model.clone(),
            effort: self.config.effort,
            permission_mode: self.config.permission_mode,
            context: self.context.clone(),
            queued: self.outbox.iter().cloned().collect(),
            problem: self.problem.clone(),
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
        let before = self.messages.len();
        let touched = fold_entry(&mut self.messages, entry);
        self.tell_messages(before, touched, outbox);
    }

    pub(super) fn fold_sent(&mut self, prompt: &Prompt, outbox: &mut Vec<EventKind>) {
        let before = self.messages.len();
        let touched = fold_prompt(&mut self.messages, prompt, self.harness);
        self.tell_messages(before, touched, outbox);
    }

    pub(super) fn replace_messages(
        &mut self,
        messages: Vec<ChatMessage>,
        outbox: &mut Vec<EventKind>,
    ) {
        let before = self.messages.len();
        self.messages = messages;
        if before > 0 {
            outbox.push(EventKind::ChatMessagesCut {
                chat: self.id.clone(),
                len: 0,
            });
        }
        self.tell_messages(0, Some(0), outbox);
    }

    pub(super) fn cut(&mut self, len: usize, outbox: &mut Vec<EventKind>) {
        let before = self.messages.len();
        self.messages.truncate(len);
        self.tell_messages(before, None, outbox);
    }

    fn tell_messages(&self, before: usize, touched: Option<usize>, outbox: &mut Vec<EventKind>) {
        for change in ChatLogChange::since(&self.messages, before, touched) {
            outbox.push(match change {
                ChatLogChange::Added { index, message } => EventKind::ChatMessageAdded {
                    chat: self.id.clone(),
                    index,
                    message: Box::new(message),
                },
                ChatLogChange::Changed { index, message } => EventKind::ChatMessageChanged {
                    chat: self.id.clone(),
                    index,
                    message: Box::new(message),
                },
                ChatLogChange::Cut { len } => EventKind::ChatMessagesCut {
                    chat: self.id.clone(),
                    len,
                },
            });
        }
    }

    pub(super) fn add_notice(&mut self, notice: &str) {
        match &mut self.problem {
            Some(shown) => {
                shown.push_str(NOTICE_SEPARATOR);
                shown.push_str(notice);
            }
            None => self.problem = Some(notice.to_owned()),
        }
    }

    pub(super) fn is_free(&self) -> bool {
        !self.working && self.asks.is_empty() && self.rewinding.is_none()
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
        self.working |= batch.message.is_some();
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
        self.working = true;
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
        self.working = false;
        true
    }

    pub(super) fn retries(&mut self, refused: &ResumeRefused) -> Option<Batch> {
        match refused {
            ResumeRefused::MissingConversation(_) => self.falls_back(),
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
