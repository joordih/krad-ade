use krad_domain::ChatEvent;
use std::thread;

use super::{Inbound, Phase, Worker, MAX_OUTPUT_TOKENS};
use crate::control::LiveEvent;
use crate::engine::attachments::Attachments;
use crate::engine::provider::stream;
use crate::engine::session::{Chosen, Record};
use crate::engine::wire::{answer_text, Message, Protocol, Request};
use crate::gateway::GatewayError;

const COMPACT_AT_PERCENT: u64 = 80;
const PERCENT: u64 = 100;
const COMPACT_SYSTEM: &str =
    "You summarize a coding session so another agent can continue it without the full history.";
const COMPACT_ASK: &str = "Summarize the conversation so far for continuing the work: the user's goals and constraints, decisions taken, files read or changed and why, commands run and their results, open problems and the next steps. Be specific with paths, names and errors. Answer with the summary only.";
const SUMMARY_INTRO: &str = "The earlier part of this conversation was compacted. Summary of it:";
const EMPTY_SUMMARY: &str = "the Gateway returned an empty summary";

fn split_point(messages: &[Message]) -> Option<usize> {
    messages
        .iter()
        .rposition(|message| matches!(message, Message::User { .. }))
        .filter(|at| *at > 0)
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn compact_if_needed(&mut self, model: &Chosen) -> bool {
        let window = self.setup.window(Some(model)).tokens();
        if self.context * PERCENT < window * COMPACT_AT_PERCENT {
            return false;
        }
        let Some(split) = split_point(&self.messages) else {
            return false;
        };
        let (attempt, cancel) = self.next_attempt();
        let access = self.setup.access.clone();
        let protocol = Protocol::from(model.vendor);
        let model = model.id.clone();
        let mut older = self.messages[..split].to_vec();
        older.push(Message::user(COMPACT_ASK.to_owned()));
        let effort = self.effort;
        let inbound = self.inbound.clone();
        self.phase = Phase::Compacting;
        thread::spawn(move || {
            let request = Request {
                model: &model,
                system: COMPACT_SYSTEM,
                messages: &older,
                images: &Attachments::read(&older),
                tools: &[],
                native: None,
                effort,
                max_tokens: MAX_OUTPUT_TOKENS,
            };
            let summary = stream(&access, protocol, &request, &cancel, &mut |_| {})
                .map(|reply| answer_text(&reply.blocks))
                .and_then(|summary| {
                    if summary.trim().is_empty() {
                        Err(GatewayError::Unreadable(EMPTY_SUMMARY.to_owned()))
                    } else {
                        Ok(summary)
                    }
                });
            inbound
                .send(Inbound::Compacted { attempt, summary })
                .unwrap_or_default();
        });
        true
    }

    pub(super) fn compacted(&mut self, summary: Result<String, GatewayError>) {
        let summary = match summary {
            Ok(summary) => summary,
            Err(GatewayError::Cancelled) => return self.finish_turn(),
            Err(error) => {
                self.phase = Phase::Idle;
                return self.emit(ChatEvent::Error(error.to_string()));
            }
        };
        let Some(split) = split_point(&self.messages) else {
            self.phase = Phase::Idle;
            return self.begin_request();
        };
        let text = format!("{SUMMARY_INTRO}\n\n{}", summary.trim());
        let kept = self.messages.len() - split;
        self.record(&Record::Compacted {
            summary: text.clone(),
            kept,
        });
        let recent = self.messages.split_off(split);
        self.messages = vec![Message::user(text)];
        self.messages.extend(recent);
        self.context = 0;
        self.phase = Phase::Idle;
        self.begin_request();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> Message {
        Message::user(text.to_owned())
    }

    #[test]
    fn compaction_keeps_the_latest_user_turn_and_what_followed_it() {
        let messages = [
            user("one"),
            user("two"),
            Message::Results { outcomes: vec![] },
        ];
        assert_eq!(split_point(&messages), Some(1));
        assert_eq!(split_point(&[user("only")]), None);
    }
}
