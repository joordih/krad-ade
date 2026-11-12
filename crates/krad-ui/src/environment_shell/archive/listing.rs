use krad_domain::{Archive, ArchivedSession, ChatHome, GroupId, LooseChat, SessionId};

pub(super) struct SessionEntry<'a> {
    pub(super) session: &'a ArchivedSession,
    pub(super) chats: Vec<&'a LooseChat>,
}

pub(super) struct OpenSessionChats<'a> {
    pub(super) session: SessionId,
    pub(super) name: &'a str,
    pub(super) chats: Vec<&'a LooseChat>,
}

#[derive(Default)]
pub(super) struct Listing<'a> {
    pub(super) sessions: Vec<SessionEntry<'a>>,
    pub(super) open: Vec<OpenSessionChats<'a>>,
    pub(super) gone: Vec<&'a LooseChat>,
}

impl Listing<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.sessions.is_empty() && self.open.is_empty() && self.gone.is_empty()
    }
}

pub(super) fn listing<'a>(archive: &'a Archive, group: GroupId, query: &str) -> Listing<'a> {
    let chats = archive.chats_of(group);
    let mut listed = Listing::default();
    for session in archive.sessions_of(group) {
        let named = session.matches(query);
        let kept: Vec<&LooseChat> = chats
            .iter()
            .copied()
            .filter(|loose| loose.home == ChatHome::Archived(session.id))
            .filter(|loose| named || loose.matches(query))
            .collect();
        if named || !kept.is_empty() {
            listed.sessions.push(SessionEntry {
                session,
                chats: kept,
            });
        }
    }
    for loose in chats.into_iter().filter(|loose| loose.matches(query)) {
        match loose.home {
            ChatHome::Open(session) => match listed
                .open
                .iter_mut()
                .find(|shelf| shelf.session == session)
            {
                Some(shelf) => shelf.chats.push(loose),
                None => listed.open.push(OpenSessionChats {
                    session,
                    name: &loose.session_name,
                    chats: vec![loose],
                }),
            },
            ChatHome::Gone => listed.gone.push(loose),
            ChatHome::Archived(_) => {}
        }
    }
    listed
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{
        AgentPreset, ArchivedChat, ArchivedSessionId, ChatBookmarks, ChatCheckpoints, ChatChoice,
        SessionLook, TerminalId,
    };
    use std::time::{Duration, SystemTime};

    const GROUP: GroupId = GroupId(1);

    fn at(hours: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_hours(hours)
    }

    fn chat(terminal: u64, title: &str) -> ArchivedChat {
        ArchivedChat {
            terminal: TerminalId(terminal),
            title: title.to_owned(),
            harness: AgentPreset::ClaudeCode,
            resume: format!("resume-{terminal}")
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid resume id")),
            choice: ChatChoice::default(),
            bookmarks: ChatBookmarks::default(),
            checkpoints: ChatCheckpoints::default(),
        }
    }

    fn session(id: u64, name: &str, closed: SystemTime) -> ArchivedSession {
        ArchivedSession {
            id: ArchivedSessionId(id),
            group: GROUP,
            name: name.to_owned(),
            look: SessionLook::default(),
            closed,
            chats: vec![chat(id * 10, "Main chat")],
            shells: Vec::new(),
            files: Vec::new(),
            lead: None,
            workers: Vec::new(),
        }
    }

    fn loose(terminal: u64, title: &str, home: ChatHome) -> LooseChat {
        LooseChat {
            group: GROUP,
            home,
            session_name: "Main".to_owned(),
            closed: at(terminal),
            chat: chat(terminal, title),
        }
    }

    fn archive() -> Archive {
        Archive {
            sessions: vec![session(1, "Refunds", at(1)), session(2, "Webhooks", at(5))],
            chats: vec![
                loose(3, "Retry plan", ChatHome::Archived(ArchivedSessionId(1))),
                loose(4, "Schema notes", ChatHome::Open(SessionId(7))),
                loose(6, "Old idea", ChatHome::Gone),
            ],
        }
    }

    #[test]
    fn loose_chats_list_under_their_session_newest_session_first() {
        let archive = archive();
        let listed = listing(&archive, GROUP, "");
        let names: Vec<&str> = listed
            .sessions
            .iter()
            .map(|entry| entry.session.name.as_str())
            .collect();
        assert_eq!(names, ["Webhooks", "Refunds"]);
        assert_eq!(listed.sessions[1].chats.len(), 1);
        assert_eq!(listed.open.len(), 1);
        assert_eq!(listed.open[0].session, SessionId(7));
        assert_eq!(listed.gone.len(), 1);
    }

    #[test]
    fn searching_keeps_a_session_whose_loose_chat_matches() {
        let archive = archive();
        let listed = listing(&archive, GROUP, "retry");
        assert_eq!(listed.sessions.len(), 1);
        assert_eq!(listed.sessions[0].session.name, "Refunds");
        assert!(listed.open.is_empty() && listed.gone.is_empty());
        assert!(listing(&archive, GROUP, "nothing like it").is_empty());
        assert!(listing(&archive, GroupId(9), "").is_empty());
    }
}
