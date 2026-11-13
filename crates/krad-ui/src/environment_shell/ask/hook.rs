use krad_client::RuntimeClient;
use krad_domain::{AskAnswer, ToolDecision};
use krad_protocol::{AskId, BusCommand, Command};
use std::sync::Arc;

pub(in crate::environment_shell) struct HookAsk {
    pub(super) ask: AskId,
    client: Arc<dyn RuntimeClient>,
}

impl HookAsk {
    pub(in crate::environment_shell) fn new(ask: AskId, client: Arc<dyn RuntimeClient>) -> Self {
        Self { ask, client }
    }

    pub(in crate::environment_shell) fn decide(self, decision: ToolDecision) {
        self.client.post(Command::Bus(BusCommand::AnswerAsk {
            ask: self.ask,
            decision,
        }));
    }

    pub(in crate::environment_shell) fn answer(self, answer: AskAnswer) {
        self.client.post(Command::Bus(BusCommand::Answer {
            ask: self.ask,
            answer,
        }));
    }
}
