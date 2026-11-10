use std::ops::Range;

use crate::chat::{ChatEntry, ChatEvent};

const BLOCK_SEPARATOR: &str = "\n\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HistorySpeaker {
    User,
    Agent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryMessage {
    pub speaker: HistorySpeaker,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct HistoryFold {
    pub(super) messages: Vec<HistoryMessage>,
}

impl HistoryFold {
    pub(super) fn first_open(&self) -> usize {
        match self.messages.last() {
            Some(last) if last.speaker == HistorySpeaker::Agent => self.messages.len() - 1,
            _ => self.messages.len(),
        }
    }

    pub(super) fn push(&mut self, entry: &ChatEntry) {
        match entry {
            ChatEntry::User { text, .. } => self.messages.push(HistoryMessage {
                speaker: HistorySpeaker::User,
                text: text.clone(),
            }),
            ChatEntry::Event(ChatEvent::Text(piece)) => self.answer(piece, true),
            ChatEntry::Event(ChatEvent::TextDelta(piece)) => self.answer(piece, false),
            ChatEntry::Event(_) | ChatEntry::Asked(_) => {}
        }
    }

    fn answer(&mut self, piece: &str, whole_block: bool) {
        if piece.is_empty() {
            return;
        }
        let open = self
            .messages
            .last_mut()
            .filter(|last| last.speaker == HistorySpeaker::Agent);
        let Some(answer) = open else {
            self.messages.push(HistoryMessage {
                speaker: HistorySpeaker::Agent,
                text: piece.to_owned(),
            });
            return;
        };
        if whole_block && !answer.text.is_empty() {
            answer.text.push_str(BLOCK_SEPARATOR);
        }
        answer.text.push_str(piece);
    }
}

#[must_use]
pub fn exchange_of(messages: &[HistoryMessage], message: usize) -> Range<usize> {
    let asked = |found: &HistoryMessage| found.speaker == HistorySpeaker::User;
    let last = message.min(messages.len().saturating_sub(1));
    let start = messages
        .get(..=last)
        .and_then(|before| before.iter().rposition(asked))
        .unwrap_or(0);
    let end = messages
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, found)| asked(found))
        .map_or(messages.len(), |(index, _)| index);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{agent_delta, agent_text, user};

    fn folded(entries: &[ChatEntry]) -> Vec<(HistorySpeaker, String)> {
        let mut fold = HistoryFold::default();
        for entry in entries {
            fold.push(entry);
        }
        fold.messages
            .into_iter()
            .map(|message| (message.speaker, message.text))
            .collect()
    }

    #[test]
    fn deltas_join_and_whole_blocks_are_separated_into_one_answer_per_turn() {
        let messages = folded(&[
            user("Why is it slow?"),
            ChatEntry::Event(ChatEvent::TurnStarted),
            agent_delta("Loo"),
            agent_delta("king"),
            agent_text("Found it."),
            ChatEntry::Event(ChatEvent::Done),
            user("Fix it"),
        ]);
        assert_eq!(
            messages,
            [
                (HistorySpeaker::User, "Why is it slow?".to_owned()),
                (HistorySpeaker::Agent, "Looking\n\nFound it.".to_owned()),
                (HistorySpeaker::User, "Fix it".to_owned()),
            ]
        );
    }

    #[test]
    fn only_an_answer_at_the_end_stays_open_to_more_text() {
        let mut fold = HistoryFold::default();
        fold.push(&user("Hi"));
        assert_eq!(fold.first_open(), 1);
        fold.push(&agent_text("Hello"));
        assert_eq!(fold.first_open(), 1);
        fold.push(&user("Bye"));
        assert_eq!(fold.first_open(), 3);
    }

    fn speakers(order: &[HistorySpeaker]) -> Vec<HistoryMessage> {
        order
            .iter()
            .map(|speaker| HistoryMessage {
                speaker: *speaker,
                text: String::new(),
            })
            .collect()
    }

    #[test]
    fn an_exchange_runs_from_its_question_to_the_next_one() {
        use HistorySpeaker::{Agent, User};
        let messages = speakers(&[User, Agent, User, Agent, Agent, User]);
        assert_eq!(exchange_of(&messages, 0), 0..2);
        assert_eq!(exchange_of(&messages, 1), 0..2);
        assert_eq!(exchange_of(&messages, 4), 2..5);
        assert_eq!(exchange_of(&messages, 5), 5..6);
        assert_eq!(exchange_of(&speakers(&[Agent, User]), 0), 0..1);
        assert_eq!(exchange_of(&[], 3), 0..0);
    }
}
