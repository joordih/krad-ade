use std::collections::HashMap;

use super::document::{ChatDocument, Posting};
use super::query::{HistoryQuery, PHRASE_SEPARATOR};
use super::snippet::{snippet, Snippet};
use super::tokens::tokens;
use crate::ids::TerminalId;

const SATURATION: f64 = 1.2;
const LENGTH_WEIGHT: f64 = 0.75;
const PREFIX_WEIGHT: f64 = 0.6;
const PHRASE_BONUS: f64 = 1.5;
const HALF: f64 = 0.5;

#[derive(Clone, Debug, PartialEq)]
pub struct ChatHit {
    pub chat: TerminalId,
    pub title: String,
    pub message: usize,
    pub score: f64,
    pub snippet: Snippet,
}

type Place = (usize, usize);

#[derive(Clone, Copy, Default)]
struct Found {
    exact: u32,
    longer: u32,
}

impl Found {
    fn weight(self) -> f64 {
        lossy(self.exact) + PREFIX_WEIGHT * lossy(self.longer)
    }
}

fn lossy(count: u32) -> f64 {
    f64::from(count)
}

fn usize_lossy(count: usize) -> f64 {
    u32::try_from(count).map_or(f64::MAX, lossy)
}

fn term_places(
    documents: &[&ChatDocument],
    query: &HistoryQuery,
    index: usize,
) -> HashMap<Place, Found> {
    let mut places: HashMap<Place, Found> = HashMap::new();
    let Some(wanted) = query.terms.get(index) else {
        return places;
    };
    for (document_index, document) in documents.iter().enumerate() {
        for (term, postings) in document.matching(wanted, query.is_prefix(index)) {
            let exact = term == wanted;
            for Posting { message, count } in postings {
                let found = places.entry((document_index, *message)).or_default();
                if exact {
                    found.exact += count;
                } else {
                    found.longer += count;
                }
            }
        }
    }
    places
}

fn inverse_frequency(total: f64, holding: f64) -> f64 {
    (1.0 + (total - holding + HALF) / (holding + HALF)).ln()
}

fn saturated(weight: f64, length: f64, average: f64) -> f64 {
    let norm = 1.0 - LENGTH_WEIGHT + LENGTH_WEIGHT * length / average.max(1.0);
    weight * (SATURATION + 1.0) / (weight + SATURATION * norm)
}

fn holds_phrase(text: &str, phrase: &str) -> bool {
    let words: Vec<String> = tokens(text).map(|token| token.term).collect();
    let wanted: Vec<&str> = phrase.split(PHRASE_SEPARATOR).collect();
    words
        .windows(wanted.len())
        .any(|window| window.iter().zip(&wanted).all(|(word, want)| word == want))
}

#[must_use]
pub fn search_chats<'a>(
    documents: impl IntoIterator<Item = &'a ChatDocument>,
    query: &HistoryQuery,
    limit: usize,
) -> Vec<ChatHit> {
    let documents: Vec<&ChatDocument> = documents.into_iter().collect();
    let total: usize = documents
        .iter()
        .map(|document| document.lengths.len())
        .sum();
    if total == 0 {
        return Vec::new();
    }
    let words: f64 = documents
        .iter()
        .flat_map(|document| document.lengths.iter())
        .map(|length| lossy(*length))
        .sum();
    let total = usize_lossy(total);
    let average = words / total;
    let per_term: Vec<HashMap<Place, Found>> = (0..query.terms.len())
        .map(|index| term_places(&documents, query, index))
        .collect();
    let Some((first, rest)) = per_term.split_first() else {
        return Vec::new();
    };
    let phrase = query.phrase();
    let mut hits: Vec<ChatHit> = first
        .keys()
        .filter(|place| rest.iter().all(|places| places.contains_key(place)))
        .filter_map(|&(document_index, message)| {
            let document = documents.get(document_index)?;
            let text = &document.messages().get(message)?.text;
            let length = document.lengths.get(message).copied().map_or(0.0, lossy);
            let mut score: f64 = per_term
                .iter()
                .filter_map(|places| {
                    let found = places.get(&(document_index, message))?;
                    let rarity = inverse_frequency(total, usize_lossy(places.len()));
                    Some(rarity * saturated(found.weight(), length, average))
                })
                .sum();
            if phrase
                .as_deref()
                .is_some_and(|phrase| holds_phrase(text, phrase))
            {
                score *= PHRASE_BONUS;
            }
            Some(ChatHit {
                chat: document.chat,
                title: document.title.clone(),
                message,
                score,
                snippet: snippet(text, query),
            })
        })
        .collect();
    let order: HashMap<TerminalId, usize> = documents
        .iter()
        .enumerate()
        .map(|(index, document)| (document.chat, index))
        .collect();
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| order.get(&left.chat).cmp(&order.get(&right.chat)))
            .then_with(|| right.message.cmp(&left.message))
    });
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::ChatEntry;
    use crate::test_support::{agent_text, user};

    const LIMIT: usize = 10;

    fn document(chat: u64, entries: &[ChatEntry]) -> ChatDocument {
        let mut document = ChatDocument::new(TerminalId(chat), format!("Chat {chat}"));
        document.extend(entries);
        document
    }

    fn places(hits: &[ChatHit]) -> Vec<(u64, usize)> {
        hits.iter()
            .map(|hit| (hit.chat.as_u64(), hit.message))
            .collect()
    }

    fn find(documents: &[ChatDocument], text: &str) -> Vec<ChatHit> {
        HistoryQuery::parse(text)
            .map_or_else(Vec::new, |query| search_chats(documents, &query, LIMIT))
    }

    #[test]
    fn every_term_has_to_be_in_the_message() {
        let documents = [document(
            1,
            &[
                user("tail reader"),
                agent_text("the reader"),
                user("tail only"),
            ],
        )];
        assert_eq!(places(&find(&documents, "tail reader ")), [(1, 0)]);
    }

    #[test]
    fn rare_terms_count_more_than_common_ones() {
        let documents = [document(
            1,
            &[
                user("deploy"),
                agent_text("deploy now"),
                user("deploy deploy deploy gateway"),
                user("deploy gateway gateway gateway"),
            ],
        )];
        assert_eq!(
            places(&find(&documents, "deploy gateway ")),
            [(1, 3), (1, 2)]
        );
    }

    #[test]
    fn a_whole_word_outranks_a_word_it_only_starts() {
        let documents = [document(1, &[user("reading notes"), user("read notes")])];
        assert_eq!(places(&find(&documents, "read")), [(1, 1), (1, 0)]);
    }

    #[test]
    fn the_terms_side_by_side_outrank_them_apart() {
        let documents = [
            document(1, &[user("tail of the reader")]),
            document(2, &[user("the tail reader too")]),
        ];
        let hits = find(&documents, "tail reader ");
        assert_eq!(places(&hits), [(2, 0), (1, 0)]);
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn ties_keep_the_chat_order_and_put_later_messages_first() {
        let documents = [
            document(4, &[user("same words"), user("same words")]),
            document(3, &[user("same words")]),
        ];
        assert_eq!(
            places(&find(&documents, "same words ")),
            [(4, 1), (4, 0), (3, 0)]
        );
    }

    #[test]
    fn hits_stop_at_the_limit_and_carry_their_snippet() {
        let documents = [document(1, &[user("one match"), user("two match")])];
        let hits = HistoryQuery::parse("match ")
            .map_or_else(Vec::new, |query| search_chats(&documents, &query, 1));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Chat 1");
        assert_eq!(hits[0].snippet.text, "two match");
    }

    #[test]
    fn nothing_indexed_finds_nothing() {
        assert_eq!(find(&[], "anything"), []);
        assert_eq!(find(&[document(1, &[])], "anything"), []);
    }
}
