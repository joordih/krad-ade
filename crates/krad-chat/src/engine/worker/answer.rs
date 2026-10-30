use krad_domain::ChatEvent;

use super::{Phase, Worker};
use crate::control::{AskAnswer, LiveEvent, Responder, ToolAsk};
use crate::engine::{plan, questions};

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn answer(&mut self, ask: &ToolAsk, answer: &AskAnswer) {
        let Phase::Tools(batch) = &mut self.phase else {
            return;
        };
        let Responder::Engine { call } = &ask.responder else {
            return;
        };
        if batch.subagents.route(call, &ask.kind, answer) {
            return;
        }
        if !batch.awaits(call) {
            return;
        }
        if batch.asks_questions() {
            let content = match answer {
                AskAnswer::Answers(answers) => questions::answered(answers),
                AskAnswer::Allow
                | AskAnswer::AllowAlways
                | AskAnswer::ApprovePlan { .. }
                | AskAnswer::Deny { .. } => questions::DISMISSED_TEXT.to_owned(),
            };
            batch.settle(content, false);
            return self.advance_batch();
        }
        if batch.asks_plan() {
            return self.answer_plan(answer);
        }
        match answer {
            AskAnswer::Allow => self.run_waiting(),
            AskAnswer::AllowAlways => {
                if let Some(action) = batch.waiting_action() {
                    self.allowed.remember(action);
                }
                self.run_waiting();
            }
            AskAnswer::Deny { message } => {
                batch.deny(message);
                self.advance_batch();
            }
            AskAnswer::ApprovePlan { .. } | AskAnswer::Answers(_) => {
                batch.deny("");
                self.advance_batch();
            }
        }
    }

    fn answer_plan(&mut self, answer: &AskAnswer) {
        let (content, mode) = match answer {
            AskAnswer::ApprovePlan { mode } => (plan::approved(*mode), Some(*mode)),
            AskAnswer::Deny { message } => (plan::rejected(message), None),
            AskAnswer::Allow | AskAnswer::AllowAlways | AskAnswer::Answers(_) => {
                (plan::rejected(""), None)
            }
        };
        if let Some(mode) = mode {
            self.mode = mode;
            self.emit(ChatEvent::PermissionMode(mode));
        }
        if let Phase::Tools(batch) = &mut self.phase {
            batch.settle(content, mode.is_none());
        }
        self.advance_batch();
    }
}
