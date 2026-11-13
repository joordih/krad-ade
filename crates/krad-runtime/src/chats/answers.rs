use krad_chat::{RewindRequest, SlashCommand};
use krad_domain::{AskAnswer, ChatEntry, Effort, ModelId, PermissionMode, Prompt, RewindScope};
use krad_protocol::{AgentRef, ChatAskId, ErrorCode, EventKind, Reply};

use super::chat::Rewinding;
use super::{ChatWork, Chats, UNKNOWN_CHAT};
use crate::runtime::refused;

const ALREADY_ANSWERED: &str = "this ask was already answered";
const UNKNOWN_ASK: &str = "no ask with this id";
const NO_REWIND_POINT: &str = "this message cannot be rewound";
const NOT_RUNNING: &str = "the chat is not running";
const BUSY: &str = "the chat is still answering";
const NOT_QUEUED: &str = "no queued message at this place";

impl Chats {
    pub(super) fn queue_prompt(&mut self, id: &AgentRef, prompt: Prompt) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        chat.outbox.push_back(prompt);
        Reply::Accepted
    }

    pub(super) fn unqueue(&mut self, id: &AgentRef, index: usize) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        match chat.outbox.remove(index) {
            Some(_) => Reply::Accepted,
            None => refused(ErrorCode::NotFound, NOT_QUEUED),
        }
    }

    pub(super) fn interrupt(&self, id: &AgentRef) -> Reply {
        let Some(live) = self.open.get(id).and_then(|chat| chat.live()) else {
            return refused(ErrorCode::NotFound, NOT_RUNNING);
        };
        live.interrupt();
        Reply::Accepted
    }

    pub(super) fn set_model(&mut self, id: &AgentRef, model: ModelId) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        if let Some(live) = chat.live() {
            live.set_model(&model);
        }
        chat.choices.model = Some(model);
        Reply::Accepted
    }

    pub(super) fn set_effort(&mut self, id: &AgentRef, effort: Effort) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        chat.choices.effort = Some(effort);
        if chat.live.is_some() {
            chat.commands.push(SlashCommand::Effort(effort));
        }
        Reply::Accepted
    }

    pub(super) fn set_thinking(&mut self, id: &AgentRef, thinking: bool) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        chat.commands.push(SlashCommand::Thinking(thinking));
        Reply::Accepted
    }

    pub(super) fn set_permission_mode(&mut self, id: &AgentRef, mode: PermissionMode) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        if let Some(live) = chat.live() {
            live.set_permission_mode(mode);
        }
        chat.choices.permission_mode = Some(mode);
        Reply::Accepted
    }

    pub(super) fn rewind(
        &mut self,
        id: &AgentRef,
        (index, prompt): (usize, Prompt),
        scope: RewindScope,
        outbox: &mut Vec<EventKind>,
    ) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        let Some(message) = chat.messages.get(index) else {
            return refused(ErrorCode::NotFound, NO_REWIND_POINT);
        };
        let Some(point) = message.rewind.clone() else {
            return refused(ErrorCode::NotFound, NO_REWIND_POINT);
        };
        if !chat.is_free() {
            return refused(ErrorCode::Unsupported, BUSY);
        }
        let Some(live) = chat.live() else {
            return refused(ErrorCode::NotFound, NOT_RUNNING);
        };
        let home = chat
            .harness
            .rewinds_from_home()
            .then(|| chat.home.clone())
            .flatten();
        live.rewind(RewindRequest { point, scope, home });
        chat.begin_work();
        chat.rewinding = Some(Rewinding { index, prompt });
        chat.tell_state(outbox);
        Reply::Accepted
    }

    pub(super) fn answer(
        &mut self,
        id: &AgentRef,
        ask: ChatAskId,
        answer: &AskAnswer,
        work: &mut ChatWork<'_>,
    ) -> Reply {
        let issued = ask.0 < self.next_ask;
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        let Some(held) = chat.asks.remove(&ask) else {
            return if issued {
                refused(ErrorCode::AlreadyAnswered, ALREADY_ANSWERED)
            } else {
                refused(ErrorCode::NotFound, UNKNOWN_ASK)
            };
        };
        if let Some(live) = chat.live() {
            live.answer(&held, answer);
        }
        let outcome = held.kind.outcome(answer.verdict(&held.kind));
        chat.fold(&ChatEntry::Asked(outcome), work.outbox);
        work.outbox.push(EventKind::ChatAskSettled {
            chat: id.clone(),
            ask,
        });
        Reply::Accepted
    }
}
