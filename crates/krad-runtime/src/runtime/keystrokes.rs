use krad_domain::AgentState;
use krad_protocol::{AgentRef, ErrorCode, Reply, Typing};
use std::time::Instant;

use super::{refused, Runtime, UNSUPPORTED_COMMAND};
use crate::terminals::Typed;
use crate::typing::{paste_bytes, Stroke, ENTER};

const UNKNOWN_TERMINAL: &str = "this runtime runs no terminal of that name";

impl Runtime {
    pub(super) fn type_into(&mut self, terminal: AgentRef, typing: Typing) -> Reply {
        if !self.terminals.knows(&terminal) {
            return refused(ErrorCode::NotFound, UNKNOWN_TERMINAL);
        }
        let now = Instant::now();
        match typing {
            Typing::Prompt { text } => self.typist.prompt(terminal, text, now),
            Typing::Submit { text } => self.submit(terminal, &text, now),
            Typing::Notice { text } => {
                self.terminals
                    .type_app(&terminal, text.into_bytes(), &mut self.outbox);
                self.typist.confirm_enter(terminal, now);
            }
            Typing::Steps { steps } => {
                let mut steps = steps.into_iter();
                if let Some(first) = steps.next() {
                    self.terminals.type_app(&terminal, first, &mut self.outbox);
                }
                self.typist.later_steps(&terminal, steps, now);
            }
            Typing::Unknown => return refused(ErrorCode::Unsupported, UNSUPPORTED_COMMAND),
        }
        Reply::Accepted
    }

    pub(super) fn submit(&mut self, terminal: AgentRef, text: &str, now: Instant) {
        self.paste(&terminal, text);
        self.typist.confirm_enter(terminal, now);
    }

    fn paste(&mut self, terminal: &AgentRef, text: &str) {
        let bracketed = self
            .terminals
            .typed(terminal)
            .is_some_and(|typed| typed.bracketed);
        self.terminals
            .type_app(terminal, paste_bytes(text, bracketed), &mut self.outbox);
    }

    fn submit_unconfirmed(&self, terminal: &AgentRef) -> bool {
        let Some(Typed {
            running: true,
            hooked: true,
            ..
        }) = self.terminals.typed(terminal)
        else {
            return false;
        };
        self.environments
            .data(&terminal.environment)
            .and_then(|data| data.nodes.iter().find(|node| node.id == terminal.terminal))
            .is_some_and(|node| node.state != AgentState::Working)
    }

    pub(super) fn type_due(&mut self, now: Instant) {
        for stroke in self.typist.due(now) {
            match stroke {
                Stroke::Keys { target, keys } => {
                    self.terminals.type_app(&target, keys, &mut self.outbox);
                }
                Stroke::Paste { target, text } => self.submit(target, &text, now),
                Stroke::Enter(enter) => {
                    if enter.is_retry && !self.submit_unconfirmed(&enter.target) {
                        continue;
                    }
                    self.terminals
                        .type_app(&enter.target, ENTER.to_vec(), &mut self.outbox);
                    if self.submit_unconfirmed(&enter.target) {
                        self.typist.retry(enter, now);
                    }
                }
            }
        }
    }
}
