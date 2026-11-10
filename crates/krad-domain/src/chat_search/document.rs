use std::collections::BTreeMap;

use super::fold::{HistoryFold, HistoryMessage};
use super::tokens::tokens;
use crate::chat::ChatEntry;
use crate::ids::TerminalId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Posting {
    pub(super) message: usize,
    pub(super) count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatDocument {
    pub chat: TerminalId,
    pub title: String,
    fold: HistoryFold,
    pub(super) postings: BTreeMap<String, Vec<Posting>>,
    pub(super) lengths: Vec<u32>,
    entries: usize,
}

impl ChatDocument {
    #[must_use]
    pub fn new(chat: TerminalId, title: String) -> Self {
        Self {
            chat,
            title,
            fold: HistoryFold::default(),
            postings: BTreeMap::new(),
            lengths: Vec::new(),
            entries: 0,
        }
    }

    #[must_use]
    pub fn messages(&self) -> &[HistoryMessage] {
        &self.fold.messages
    }

    #[must_use]
    pub const fn entries(&self) -> usize {
        self.entries
    }

    pub fn extend(&mut self, entries: &[ChatEntry]) {
        if entries.is_empty() {
            return;
        }
        let first = self.fold.first_open();
        for entry in entries {
            self.fold.push(entry);
        }
        self.entries += entries.len();
        self.reindex_from(first);
    }

    fn reindex_from(&mut self, first: usize) {
        for postings in self.postings.values_mut() {
            postings.retain(|posting| posting.message < first);
        }
        self.postings.retain(|_, postings| !postings.is_empty());
        self.lengths.truncate(first);
        for (message, found) in self.fold.messages.iter().enumerate().skip(first) {
            let mut counts: BTreeMap<String, u32> = BTreeMap::new();
            let mut length = 0_u32;
            for token in tokens(&found.text) {
                *counts.entry(token.term).or_default() += 1;
                length += 1;
            }
            self.lengths.push(length);
            for (term, count) in counts {
                self.postings
                    .entry(term)
                    .or_default()
                    .push(Posting { message, count });
            }
        }
    }

    pub(super) fn matching<'a>(
        &'a self,
        wanted: &'a str,
        prefix: bool,
    ) -> impl Iterator<Item = (&'a str, &'a [Posting])> + 'a {
        self.postings
            .range::<str, _>((
                std::ops::Bound::Included(wanted),
                std::ops::Bound::Unbounded,
            ))
            .take_while(move |(term, _)| {
                if prefix {
                    term.starts_with(wanted)
                } else {
                    term.as_str() == wanted
                }
            })
            .map(|(term, postings)| (term.as_str(), postings.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{agent_delta, agent_text, user};

    fn messages_with(document: &ChatDocument, term: &str) -> Vec<(usize, u32)> {
        document
            .matching(term, false)
            .flat_map(|(_, postings)| postings.iter())
            .map(|posting| (posting.message, posting.count))
            .collect()
    }

    #[test]
    fn each_term_lists_the_messages_it_is_in_with_how_often() {
        let mut document = ChatDocument::new(TerminalId(1), "Chat".to_owned());
        document.extend(&[
            user("Race race condition"),
            agent_text("The race is in the reader"),
        ]);
        assert_eq!(messages_with(&document, "race"), [(0, 2), (1, 1)]);
        assert_eq!(messages_with(&document, "reader"), [(1, 1)]);
        assert_eq!(document.lengths, [3, 6]);
        assert_eq!(document.entries(), 2);
    }

    #[test]
    fn new_entries_only_reindex_the_answer_they_continue() {
        let mut whole = ChatDocument::new(TerminalId(1), "Chat".to_owned());
        let entries = [
            user("Look at the tail"),
            agent_delta("The tail rea"),
            agent_delta("der skips lines"),
            user("Thanks"),
        ];
        whole.extend(&entries);
        let mut grown = ChatDocument::new(TerminalId(1), "Chat".to_owned());
        grown.extend(&entries[..2]);
        assert_eq!(messages_with(&grown, "rea"), [(1, 1)]);
        grown.extend(&entries[2..]);
        assert_eq!(grown, whole);
        assert_eq!(messages_with(&grown, "rea"), []);
        assert_eq!(messages_with(&grown, "reader"), [(1, 1)]);
    }

    #[test]
    fn a_prefix_reaches_every_longer_term_and_an_exact_term_only_itself() {
        let mut document = ChatDocument::new(TerminalId(1), "Chat".to_owned());
        document.extend(&[user("read reader reads ready")]);
        let terms = |prefix| -> Vec<&str> {
            document
                .matching("read", prefix)
                .map(|(term, _)| term)
                .collect()
        };
        assert_eq!(terms(true), ["read", "reader", "reads", "ready"]);
        assert_eq!(terms(false), ["read"]);
    }
}
