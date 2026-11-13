use krad_chat::{
    claude_conversation_file, read_claude_transcript_before, read_claude_transcript_tail,
    read_codex_transcript, read_engine_transcript, transcript_started,
};
use krad_domain::{fold_entry, last_permission_mode, ChatEntry, ChatMessage, ChatOrigin, ResumeId};
use krad_host::HostLink;
use krad_protocol::{AgentRef, ErrorCode, Reply};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::backend::HistorySource;
use super::chat::{Chat, Earlier, Shown};
use super::{ChatWork, Chats, UNKNOWN_CHAT};
use crate::jobs::{JobDone, Jobs};
use crate::runtime::refused;

const FIRST_LOOK_BYTES: u64 = 2 * 1024 * 1024;
const NO_HOME_TEXT: &str = "Cannot find the home folder Claude Code keeps conversations in";
const NO_CONVERSATION_TEXT: &str = "Cannot find the conversation to resume";
const NO_EARLIER: &str = "there is no earlier history to show";

#[derive(Debug)]
pub(crate) struct HistoryRead {
    entries: Vec<ChatEntry>,
    earlier: Option<Earlier>,
    started: Option<SystemTime>,
}

#[derive(Debug)]
pub(crate) struct ChatOpening {
    pub(crate) chat: AgentRef,
    pub(crate) opened: u64,
    pub(crate) home: Option<PathBuf>,
    pub(crate) history: Option<Result<HistoryRead, String>>,
}

#[derive(Debug)]
pub(crate) struct ChatEarlier {
    pub(crate) chat: AgentRef,
    pub(crate) opened: u64,
    pub(crate) read: Result<(Vec<ChatEntry>, Option<SystemTime>), String>,
}

pub(super) fn read(chat: &Chat, jobs: &Jobs) {
    let source = HistorySource::of(chat);
    let resume = chat
        .messages
        .is_empty()
        .then(|| chat.session.history().cloned())
        .flatten();
    let (id, opened) = (chat.id.clone(), chat.opened);
    let started = jobs.run(move || {
        let (home, history) = look(&source, resume.as_ref());
        JobDone::ChatOpened(ChatOpening {
            chat: id,
            opened,
            home,
            history,
        })
    });
    if let Err(error) = started {
        tracing::warn!(%error, "the chat history job could not start");
    }
}

fn look(
    source: &HistorySource,
    resume: Option<&ResumeId>,
) -> (Option<PathBuf>, Option<Result<HistoryRead, String>>) {
    let whole = |entries: Vec<ChatEntry>| HistoryRead {
        entries,
        earlier: None,
        started: None,
    };
    match source {
        HistorySource::Claude { link, root } => look_claude(link, root.as_deref(), resume),
        HistorySource::Codex { runner, cwd } => (
            None,
            resume.map(|resume| {
                read_codex_transcript(runner, cwd, resume)
                    .map(whole)
                    .map_err(|error| error.to_string())
            }),
        ),
        HistorySource::Gateway { sessions, cwd } => (
            None,
            resume.map(|resume| {
                read_engine_transcript(sessions, cwd, resume)
                    .map(whole)
                    .map_err(|error| error.to_string())
            }),
        ),
        HistorySource::Unread => (None, None),
    }
}

fn look_claude(
    link: &HostLink,
    root: Option<&Path>,
    resume: Option<&ResumeId>,
) -> (Option<PathBuf>, Option<Result<HistoryRead, String>>) {
    let home = krad_repo::workplace_home(link, root).map_err(|error| error.to_string());
    let history = resume.map(|resume| {
        let home = home.clone()?.ok_or_else(|| NO_HOME_TEXT.to_owned())?;
        let file = claude_conversation_file(link, &home, resume)
            .ok_or_else(|| NO_CONVERSATION_TEXT.to_owned())?;
        let (chunk, start) = read_claude_transcript_tail(link, &file, FIRST_LOOK_BYTES)
            .map_err(|error| error.to_string())?;
        let started = transcript_started(link, &file);
        Ok(HistoryRead {
            entries: chunk.entries,
            earlier: (start > 0).then_some(Earlier { file, end: start }),
            started,
        })
    });
    (home.ok().flatten(), history)
}

fn folded(entries: &[ChatEntry]) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    for entry in entries {
        fold_entry(&mut messages, entry);
    }
    if let Some(last) = messages.last_mut() {
        last.running = false;
    }
    messages
}

impl Chats {
    pub(crate) fn opened(&mut self, opening: ChatOpening, work: &mut ChatWork<'_>) {
        let Some(chat) = self.open.get_mut(&opening.chat) else {
            return;
        };
        if chat.opened != opening.opened {
            return;
        }
        chat.home = opening.home;
        match opening.history {
            Some(Ok(read)) => {
                if chat.choices.permission_mode.is_none() {
                    chat.choices.permission_mode = last_permission_mode(&read.entries);
                }
                if read.earlier.is_none() {
                    chat.origin = ChatOrigin::of(&read.entries, read.started);
                }
                chat.shown = read.earlier.map_or(Shown::Whole, Shown::FromTail);
                let mut messages = folded(&read.entries);
                messages.append(&mut chat.messages);
                chat.replace_messages(messages, work.outbox);
            }
            Some(Err(problem)) => chat.problem = Some(problem),
            None => {}
        }
        self.flush(&opening.chat, work);
    }

    pub(super) fn show_earlier(&mut self, id: &AgentRef, jobs: &Jobs) -> Reply {
        let Some(chat) = self.open.get_mut(id) else {
            return refused(ErrorCode::NotFound, UNKNOWN_CHAT);
        };
        let earlier = match &chat.shown {
            Shown::Whole => return refused(ErrorCode::NotFound, NO_EARLIER),
            Shown::ReadingEarlier(_) => return Reply::Accepted,
            Shown::FromTail(earlier) => earlier.clone(),
        };
        let reading = earlier.clone();
        let link = chat.place.link.clone();
        let (chat_id, opened) = (chat.id.clone(), chat.opened);
        let started = jobs.run(move || {
            let read = read_claude_transcript_before(&link, &earlier.file, earlier.end)
                .map(|entries| (entries, transcript_started(&link, &earlier.file)))
                .map_err(|error| error.to_string());
            JobDone::ChatEarlier(ChatEarlier {
                chat: chat_id,
                opened,
                read,
            })
        });
        match started {
            Ok(()) => {
                chat.shown = Shown::ReadingEarlier(reading);
                Reply::Accepted
            }
            Err(error) => refused(ErrorCode::Unsupported, &error.to_string()),
        }
    }

    pub(crate) fn earlier_read(&mut self, earlier: ChatEarlier, work: &mut ChatWork<'_>) {
        let Some(chat) = self.open.get_mut(&earlier.chat) else {
            return;
        };
        if chat.opened != earlier.opened {
            return;
        }
        let reading = std::mem::take(&mut chat.shown);
        match earlier.read {
            Ok((entries, started)) => {
                chat.origin = ChatOrigin::of(&entries, started);
                chat.prepend(folded(&entries), work.outbox);
            }
            Err(problem) => {
                if let Shown::ReadingEarlier(unread) = reading {
                    chat.shown = Shown::FromTail(unread);
                }
                chat.problem = Some(problem);
            }
        }
        self.flush(&earlier.chat, work);
    }
}
