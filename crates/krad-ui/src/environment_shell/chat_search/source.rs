use krad_domain::{ChatDocument, ChatEntry, TerminalId};
use krad_protocol::{HistoryCursor, HistoryPart};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub(super) struct Indexed {
    pub(super) document: Option<Arc<ChatDocument>>,
    pub(super) cursor: HistoryCursor,
    pub(super) last: Option<ChatEntry>,
}

fn continues(document: &ChatDocument, last: Option<&ChatEntry>, entries: &[ChatEntry]) -> bool {
    let seen = document.entries();
    seen <= entries.len()
        && seen
            .checked_sub(1)
            .is_none_or(|index| entries.get(index) == last)
}

pub(super) fn merged(
    indexed: Indexed,
    chat: TerminalId,
    title: String,
    read: HistoryPart,
) -> Indexed {
    let Indexed {
        document,
        cursor,
        last,
    } = indexed;
    let mut document = document.map_or_else(
        || ChatDocument::new(chat, title.clone()),
        Arc::unwrap_or_clone,
    );
    document.title = title;
    let (fresh, cursor, last) = match read {
        HistoryPart::Appended { entries, cursor } => {
            let last = entries.last().cloned().or(last);
            (entries, cursor, last)
        }
        HistoryPart::Whole { mut entries } => {
            if !continues(&document, last.as_ref(), &entries) {
                document = ChatDocument::new(chat, document.title.clone());
            }
            let last = entries.last().cloned();
            let fresh = entries.split_off(document.entries().min(entries.len()));
            (fresh, cursor, last)
        }
    };
    document.extend(&fresh);
    Indexed {
        document: Some(Arc::new(document)),
        cursor,
        last,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, ChatEvent, HistoryMessage, HistorySpeaker};

    fn user(text: &str) -> ChatEntry {
        ChatEntry::User {
            text: text.to_owned(),
            preset: AgentPreset::Gateway,
            model: None,
            images: Vec::new(),
        }
    }

    fn answer(text: &str) -> ChatEntry {
        ChatEntry::Event(ChatEvent::Text(text.to_owned()))
    }

    fn texts(indexed: &Indexed) -> Vec<String> {
        indexed
            .document
            .as_deref()
            .map_or(&[][..], ChatDocument::messages)
            .iter()
            .map(|message: &HistoryMessage| message.text.clone())
            .collect()
    }

    fn whole(indexed: Indexed, entries: &[ChatEntry]) -> Indexed {
        merged(
            indexed,
            TerminalId(1),
            "Chat".to_owned(),
            HistoryPart::Whole {
                entries: entries.to_vec(),
            },
        )
    }

    #[test]
    fn a_grown_transcript_only_adds_what_is_new() {
        let first = whole(Indexed::default(), &[user("one"), answer("two")]);
        let grown = whole(first, &[user("one"), answer("two"), user("three")]);
        assert_eq!(texts(&grown), ["one", "two", "three"]);
        assert_eq!(
            grown.document.as_ref().map(|document| document.entries()),
            Some(3)
        );
    }

    #[test]
    fn a_rewound_transcript_is_indexed_again_from_the_start() {
        let first = whole(Indexed::default(), &[user("one"), answer("two")]);
        let cut = whole(first, &[user("one"), answer("other"), user("again")]);
        assert_eq!(texts(&cut), ["one", "other", "again"]);
        let shorter = whole(cut, &[user("only")]);
        assert_eq!(texts(&shorter), ["only"]);
    }

    #[test]
    fn an_appended_read_moves_the_cursor_and_keeps_the_new_title() {
        let cursor = HistoryCursor {
            next: 42,
            ..HistoryCursor::default()
        };
        let read = HistoryPart::Appended {
            entries: vec![user("hi")],
            cursor: cursor.clone(),
        };
        let indexed = merged(
            Indexed::default(),
            TerminalId(1),
            "Renamed".to_owned(),
            read,
        );
        assert_eq!(indexed.cursor, cursor);
        assert_eq!(
            indexed
                .document
                .as_ref()
                .map(|document| document.title.as_str()),
            Some("Renamed")
        );
        let speakers: Vec<HistorySpeaker> = indexed
            .document
            .as_deref()
            .map_or(&[][..], ChatDocument::messages)
            .iter()
            .map(|message| message.speaker)
            .collect();
        assert_eq!(speakers, [HistorySpeaker::User]);
    }
}
