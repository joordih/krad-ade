use futures::channel::oneshot;
use krad_agents::Response;
use krad_chat::ToolAsk;
use krad_domain::{AskAnswer, ToolDecision};
use krad_protocol::{
    AgentRef, AskId, AskRequest, AskSettlement, ErrorCode, EventKind, PendingAsk, Reply,
};
use std::collections::BTreeMap;

use super::reply_with;

const ALREADY_ANSWERED: &str = "the ask was already answered or its caller left";
const NO_SUCH_ASK: &str = "no ask with that id";

#[derive(Debug)]
struct Held {
    pending: PendingAsk,
    asked: ToolAsk,
    reply: oneshot::Sender<Response>,
}

#[derive(Debug, Default)]
pub(crate) struct Asks {
    last: AskId,
    held: BTreeMap<AskId, Held>,
}

impl Asks {
    pub(crate) fn hold(
        &mut self,
        agent: AgentRef,
        request: AskRequest,
        reply: oneshot::Sender<Response>,
        outbox: &mut Vec<EventKind>,
    ) {
        self.last = self.last.next();
        let asked = ToolAsk::of_hook(
            request.tool.clone(),
            request.input.clone(),
            request.suggestions.clone(),
        );
        let pending = PendingAsk {
            ask: self.last,
            agent,
            request,
            kind: asked.kind.clone(),
            can_allow_always: asked.can_allow_always(),
        };
        outbox.push(EventKind::AskPending(pending.clone()));
        self.held.insert(
            self.last,
            Held {
                pending,
                asked,
                reply,
            },
        );
    }

    pub(crate) fn answer_with(
        &mut self,
        ask: AskId,
        answer: &AskAnswer,
        outbox: &mut Vec<EventKind>,
    ) -> Reply {
        let Some(decision) = self.held.get(&ask).map(|held| held.asked.decision(answer)) else {
            return self.refusal(ask);
        };
        self.answer(ask, &decision, outbox)
    }

    pub(crate) fn answer(
        &mut self,
        ask: AskId,
        decision: &ToolDecision,
        outbox: &mut Vec<EventKind>,
    ) -> Reply {
        let Some(held) = self.held.remove(&ask) else {
            return self.refusal(ask);
        };
        reply_with(held.reply, Response::Decided(decision.clone()));
        outbox.push(EventKind::AskSettled {
            ask,
            settlement: AskSettlement::Answered,
        });
        Reply::Accepted
    }

    pub(crate) fn tick(&mut self, outbox: &mut Vec<EventKind>) {
        let left: Vec<AskId> = self
            .held
            .iter()
            .filter(|(_, held)| held.reply.is_canceled())
            .map(|(ask, _)| *ask)
            .collect();
        for ask in left {
            self.held.remove(&ask);
            outbox.push(EventKind::AskSettled {
                ask,
                settlement: AskSettlement::CallerLeft,
            });
        }
    }

    pub(crate) fn pending(&self) -> Vec<PendingAsk> {
        self.held
            .values()
            .map(|held| held.pending.clone())
            .collect()
    }

    fn refusal(&self, ask: AskId) -> Reply {
        let issued = ask > AskId::default() && ask <= self.last;
        let (code, message) = if issued {
            (ErrorCode::AlreadyAnswered, ALREADY_ANSWERED)
        } else {
            (ErrorCode::NotFound, NO_SUCH_ASK)
        };
        Reply::Refused {
            code,
            message: message.to_owned(),
        }
    }
}
