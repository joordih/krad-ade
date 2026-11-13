use krad_chat::{claude_conversation_file, read_claude_transcript_tail};
use krad_domain::{fold_entry, last_permission_mode, ChatEntry, ChatMessage, ResumeId};
use krad_host::HostLink;
use krad_protocol::AgentRef;
use std::path::{Path, PathBuf};

use super::chat::Chat;
use super::{ChatWork, Chats};
use crate::jobs::{JobDone, Jobs};

const FIRST_LOOK_BYTES: u64 = 2 * 1024 * 1024;
const NO_HOME_TEXT: &str = "Cannot find the home folder Claude Code keeps conversations in";
const NO_CONVERSATION_TEXT: &str = "Cannot find the conversation to resume";

#[derive(Debug)]
pub(crate) struct ChatOpening {
    pub(crate) chat: AgentRef,
    pub(crate) opened: u64,
    pub(crate) home: Option<PathBuf>,
    pub(crate) history: Option<Result<Vec<ChatEntry>, String>>,
}

pub(super) fn read(chat: &Chat, jobs: &Jobs) {
    let link = chat.place.link.clone();
    let root = chat.place.root.clone();
    let resume = chat
        .messages
        .is_empty()
        .then(|| chat.session.history().cloned())
        .flatten();
    let id = chat.id.clone();
    let opened = chat.opened;
    let started = jobs.run(move || {
        JobDone::ChatOpened(look(id, opened, &link, root.as_deref(), resume.as_ref()))
    });
    if let Err(error) = started {
        tracing::warn!(%error, "the chat history job could not start");
    }
}

fn look(
    chat: AgentRef,
    opened: u64,
    link: &HostLink,
    root: Option<&Path>,
    resume: Option<&ResumeId>,
) -> ChatOpening {
    let home = krad_repo::workplace_home(link, root).map_err(|error| error.to_string());
    let history = resume.map(|resume| {
        let home = home.clone()?.ok_or_else(|| NO_HOME_TEXT.to_owned())?;
        let file = claude_conversation_file(link, &home, resume)
            .ok_or_else(|| NO_CONVERSATION_TEXT.to_owned())?;
        read_claude_transcript_tail(link, &file, FIRST_LOOK_BYTES)
            .map(|(chunk, _)| chunk.entries)
            .map_err(|error| error.to_string())
    });
    ChatOpening {
        chat,
        opened,
        home: home.ok().flatten(),
        history,
    }
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
            Some(Ok(entries)) => {
                if chat.config.permission_mode.is_none() {
                    chat.config.permission_mode = last_permission_mode(&entries);
                }
                let mut messages = folded(&entries);
                messages.append(&mut chat.messages);
                chat.replace_messages(messages, work.outbox);
            }
            Some(Err(problem)) => chat.problem = Some(problem),
            None => {}
        }
        self.flush(&opening.chat, work.outbox);
    }
}
