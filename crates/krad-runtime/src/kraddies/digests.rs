use krad_chat::{ChatEvent, LiveChat, LiveEvent};
use krad_domain::{DigestChat, MemoryOwner};
use krad_protocol::AgentRef;
use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

use crate::input::Input;

pub(crate) const DIGEST_CHECK_EVERY: Duration = Duration::from_mins(1);
pub(crate) const DIGEST_TIMEOUT: Duration = Duration::from_mins(4);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DigestReply {
    Text(String),
    Failed(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Watched {
    pub(crate) owner: MemoryOwner,
    pub(crate) chat: DigestChat,
}

pub(crate) struct DigestWriter {
    pub(crate) watched: Watched,
    pub(crate) covered: usize,
    pub(crate) started: Instant,
    pub(crate) live: Option<LiveChat>,
}

impl fmt::Debug for DigestWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DigestWriter")
            .field("owner", &self.watched.owner)
            .field("covered", &self.covered)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(crate) struct DigestWatch {
    pub(crate) quiet: HashMap<AgentRef, Instant>,
    pub(crate) attempted: HashMap<AgentRef, usize>,
    pub(crate) known: HashMap<AgentRef, Watched>,
    pub(crate) writing: HashMap<AgentRef, DigestWriter>,
    pub(crate) next_check: Option<Instant>,
}

impl DigestWatch {
    pub(crate) fn check_due(&mut self, now: Instant) -> bool {
        if self.next_check.is_some_and(|next| now < next) {
            return false;
        }
        self.next_check = Some(now + DIGEST_CHECK_EVERY);
        true
    }

    pub(crate) fn overdue(&self, now: Instant) -> Vec<AgentRef> {
        self.writing
            .iter()
            .filter(|(_, writer)| now.saturating_duration_since(writer.started) >= DIGEST_TIMEOUT)
            .map(|(chat, _)| chat.clone())
            .collect()
    }
}

pub(crate) fn reply_sink(
    chat: AgentRef,
    inbox: SyncSender<Input>,
) -> impl FnMut(LiveEvent) + Send + 'static {
    let mut chat = Some(chat);
    let mut whole = String::new();
    let mut pieces = String::new();
    move |event| {
        let finished = match event {
            LiveEvent::Chat(ChatEvent::Text(text)) => {
                whole.push_str(&text);
                whole.push('\n');
                None
            }
            LiveEvent::Chat(ChatEvent::TextDelta(text)) => {
                pieces.push_str(&text);
                None
            }
            LiveEvent::Chat(ChatEvent::Done) => {
                let text = if whole.trim().is_empty() {
                    &mut pieces
                } else {
                    &mut whole
                };
                Some(DigestReply::Text(std::mem::take(text)))
            }
            LiveEvent::Chat(ChatEvent::Error(error)) => Some(DigestReply::Failed(error)),
            _ => None,
        };
        let Some(reply) = finished else {
            return;
        };
        let Some(chat) = chat.take() else {
            return;
        };
        if inbox.send(Input::DigestHeard { chat, reply }).is_err() {
            tracing::trace!("a chat digest arrived after the runtime stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{EnvironmentName, TerminalId};
    use std::sync::mpsc::sync_channel;

    #[test]
    fn the_writer_keeps_the_whole_answer_or_its_pieces_once() {
        let (inbox, heard) = sync_channel(4);
        let chat = AgentRef {
            environment: EnvironmentName::new("main".to_owned()),
            terminal: TerminalId(3),
        };
        let mut sink = reply_sink(chat.clone(), inbox);
        sink(LiveEvent::Chat(ChatEvent::TextDelta("- a".to_owned())));
        sink(LiveEvent::Chat(ChatEvent::TextDelta("\n- b".to_owned())));
        sink(LiveEvent::Chat(ChatEvent::Done));
        sink(LiveEvent::Chat(ChatEvent::Done));
        let heard: Vec<Input> = heard.try_iter().collect();
        assert!(matches!(
            heard.as_slice(),
            [Input::DigestHeard { chat: from, reply: DigestReply::Text(text) }]
                if *from == chat && text == "- a\n- b"
        ));
    }
}
