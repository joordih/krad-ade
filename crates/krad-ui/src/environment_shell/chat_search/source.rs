use krad_chat::{
    claude_conversation_file, read_claude_transcript, read_codex_transcript,
    read_engine_transcript, read_opencode_transcript, OpencodeServer, Runner,
};
use krad_domain::{ChatDocument, ChatEntry, ResumeId, TerminalId};
use krad_host::HostLink;
use krad_store::read_acp_transcript;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::chat::workplace_home;

const NO_HOME_TEXT: &str = "Cannot find the home folder Claude Code keeps conversations in";
const NO_CONVERSATION_TEXT: &str = "Cannot find the conversation file of this chat";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ClaudeCursor {
    home: Option<PathBuf>,
    file: Option<PathBuf>,
    next: u64,
}

pub(super) enum HistorySource {
    Claude {
        link: HostLink,
        resume: ResumeId,
        root: Option<PathBuf>,
        cursor: ClaudeCursor,
    },
    Gateway {
        sessions: PathBuf,
        cwd: PathBuf,
        resume: ResumeId,
    },
    Acp {
        file: PathBuf,
    },
    Codex {
        runner: Runner,
        cwd: PathBuf,
        resume: ResumeId,
    },
    Opencode {
        server: Arc<OpencodeServer>,
        resume: ResumeId,
    },
}

pub(super) enum HistoryRead {
    Appended(Vec<ChatEntry>, ClaudeCursor),
    Whole(Vec<ChatEntry>),
}

fn read_claude(
    link: &HostLink,
    resume: &ResumeId,
    root: Option<&Path>,
    mut cursor: ClaudeCursor,
) -> Result<HistoryRead, String> {
    if cursor.home.is_none() {
        cursor.home = workplace_home(link, root).map_err(|error| error.to_string())?;
    }
    let home = cursor.home.clone().ok_or_else(|| NO_HOME_TEXT.to_owned())?;
    if cursor.file.is_none() {
        cursor.file = claude_conversation_file(link, &home, resume);
    }
    let file = cursor
        .file
        .clone()
        .ok_or_else(|| NO_CONVERSATION_TEXT.to_owned())?;
    let chunk =
        read_claude_transcript(link, &file, cursor.next).map_err(|error| error.to_string())?;
    cursor.next = chunk.next;
    Ok(HistoryRead::Appended(chunk.entries, cursor))
}

impl HistorySource {
    pub(super) fn read(self) -> Result<HistoryRead, String> {
        match self {
            Self::Claude {
                link,
                resume,
                root,
                cursor,
            } => read_claude(&link, &resume, root.as_deref(), cursor),
            Self::Gateway {
                sessions,
                cwd,
                resume,
            } => read_engine_transcript(&sessions, &cwd, &resume)
                .map(HistoryRead::Whole)
                .map_err(|error| error.to_string()),
            Self::Acp { file } => read_acp_transcript(&file)
                .map(HistoryRead::Whole)
                .map_err(|error| error.to_string()),
            Self::Codex {
                runner,
                cwd,
                resume,
            } => read_codex_transcript(&runner, &cwd, &resume)
                .map(HistoryRead::Whole)
                .map_err(|error| error.to_string()),
            Self::Opencode { server, resume } => read_opencode_transcript(&server, &resume)
                .map(HistoryRead::Whole)
                .map_err(|error| error.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Indexed {
    pub(super) document: Option<Arc<ChatDocument>>,
    pub(super) cursor: ClaudeCursor,
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
    read: HistoryRead,
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
        HistoryRead::Appended(entries, cursor) => {
            let last = entries.last().cloned().or(last);
            (entries, cursor, last)
        }
        HistoryRead::Whole(mut entries) => {
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
            HistoryRead::Whole(entries.to_vec()),
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
        let cursor = ClaudeCursor {
            next: 42,
            ..ClaudeCursor::default()
        };
        let read = HistoryRead::Appended(vec![user("hi")], cursor.clone());
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
