use super::{
    Archive, ArchiveRetention, ArchivedChat, ArchivedSession, ArchivedSessionId, ChatHome,
    LooseChat,
};
use crate::ids::{GroupId, SessionId, TerminalId};
use std::cmp::Reverse;
use std::time::SystemTime;

impl Archive {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty() && self.chats.is_empty()
    }

    #[must_use]
    pub fn holds(&self, terminal: TerminalId) -> bool {
        self.sessions
            .iter()
            .flat_map(ArchivedSession::all_chats)
            .chain(self.chats.iter().map(|loose| &loose.chat))
            .any(|chat| chat.terminal == terminal)
    }

    #[must_use]
    pub fn sessions_of(&self, group: GroupId) -> Vec<&ArchivedSession> {
        let mut sessions: Vec<&ArchivedSession> = self
            .sessions
            .iter()
            .filter(|session| session.group == group)
            .collect();
        sessions.sort_by_key(|session| Reverse(session.closed));
        sessions
    }

    #[must_use]
    pub fn chats_of(&self, group: GroupId) -> Vec<&LooseChat> {
        let mut chats: Vec<&LooseChat> = self
            .chats
            .iter()
            .filter(|loose| loose.group == group)
            .collect();
        chats.sort_by_key(|loose| Reverse(loose.closed));
        chats
    }

    #[must_use]
    pub fn session_holding(&self, terminal: TerminalId) -> Option<ArchivedSessionId> {
        self.sessions
            .iter()
            .find(|session| session.all_chats().any(|chat| chat.terminal == terminal))
            .map(|session| session.id)
    }

    pub(super) fn next_session_id(&self) -> ArchivedSessionId {
        ArchivedSessionId(
            self.sessions
                .iter()
                .map(|session| session.id.as_u64())
                .max()
                .map_or(1, |last| last + 1),
        )
    }

    pub fn take_session(&mut self, id: ArchivedSessionId) -> Option<ArchivedSession> {
        let at = self.sessions.iter().position(|session| session.id == id)?;
        Some(self.sessions.remove(at))
    }

    pub fn take_chat(&mut self, terminal: TerminalId) -> Option<LooseChat> {
        let at = self
            .chats
            .iter()
            .position(|loose| loose.chat.terminal == terminal)?;
        Some(self.chats.remove(at))
    }

    pub fn rehome_chats(&mut self, from: ChatHome, to: ChatHome) {
        for loose in self.chats.iter_mut().filter(|loose| loose.home == from) {
            loose.home = to;
        }
    }

    pub fn leave_session(&mut self, session: SessionId) {
        self.rehome_chats(ChatHome::Open(session), ChatHome::Gone);
    }

    pub fn delete_session(&mut self, id: ArchivedSessionId) -> Vec<ArchivedChat> {
        let mut deleted: Vec<ArchivedChat> = self
            .take_session(id)
            .map_or_default(|session| session.chats.into_iter().chain(session.workers).collect());
        deleted.extend(self.drain_chats(|loose| loose.home == ChatHome::Archived(id)));
        deleted
    }

    pub fn delete_chat(&mut self, terminal: TerminalId) -> Option<ArchivedChat> {
        self.take_chat(terminal).map(|loose| loose.chat)
    }

    pub fn forget_group(&mut self, group: GroupId) -> Vec<ArchivedChat> {
        let ids: Vec<ArchivedSessionId> = self
            .sessions
            .iter()
            .filter(|session| session.group == group)
            .map(|session| session.id)
            .collect();
        let mut deleted: Vec<ArchivedChat> = ids
            .into_iter()
            .flat_map(|id| self.delete_session(id))
            .collect();
        deleted.extend(self.drain_chats(|loose| loose.group == group));
        deleted
    }

    pub fn forget_all(&mut self) -> Vec<ArchivedChat> {
        let sessions = std::mem::take(&mut self.sessions);
        let chats = std::mem::take(&mut self.chats);
        sessions
            .into_iter()
            .flat_map(|session| session.chats.into_iter().chain(session.workers))
            .chain(chats.into_iter().map(|loose| loose.chat))
            .collect()
    }

    pub fn expire(&mut self, retention: ArchiveRetention, now: SystemTime) -> Vec<ArchivedChat> {
        let expired: Vec<ArchivedSessionId> = self
            .sessions
            .iter()
            .filter(|session| retention.has_expired(session.closed, now))
            .map(|session| session.id)
            .collect();
        let mut deleted = Vec::new();
        for id in expired {
            if let Some(session) = self.take_session(id) {
                deleted.extend(session.chats.into_iter().chain(session.workers));
            }
            self.rehome_chats(ChatHome::Archived(id), ChatHome::Gone);
        }
        deleted.extend(self.drain_chats(|loose| retention.has_expired(loose.closed, now)));
        deleted
    }

    fn drain_chats(&mut self, leaves: impl Fn(&LooseChat) -> bool) -> Vec<ArchivedChat> {
        let (gone, kept): (Vec<LooseChat>, Vec<LooseChat>) = std::mem::take(&mut self.chats)
            .into_iter()
            .partition(leaves);
        self.chats = kept;
        gone.into_iter().map(|loose| loose.chat).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{chat, now, session};
    use super::*;
    use std::time::Duration;

    const GROUP: GroupId = GroupId(1);
    const OTHER: GroupId = GroupId(2);

    fn loose(terminal: u64, group: GroupId, home: ChatHome, closed: SystemTime) -> LooseChat {
        LooseChat {
            group,
            home,
            session_name: "Main".to_owned(),
            closed,
            chat: chat(terminal, "Loose"),
        }
    }

    fn days(count: u64) -> Duration {
        Duration::from_hours(24 * count)
    }

    #[test]
    fn expiring_removes_only_what_is_past_the_retention() {
        let mut archive = Archive::default();
        archive.sessions.push(session(1, GROUP, now() - days(40)));
        archive.sessions.push(session(2, GROUP, now() - days(2)));
        archive.chats.push(loose(
            7,
            GROUP,
            ChatHome::Archived(ArchivedSessionId(1)),
            now() - days(3),
        ));
        archive
            .chats
            .push(loose(8, GROUP, ChatHome::Gone, now() - days(31)));
        let deleted = archive.expire(ArchiveRetention::ThirtyDays, now());
        let gone: Vec<TerminalId> = deleted.iter().map(|chat| chat.terminal).collect();
        assert_eq!(gone, [TerminalId(10), TerminalId(8)]);
        assert_eq!(archive.sessions.len(), 1);
        assert_eq!(archive.sessions[0].id, ArchivedSessionId(2));
        assert_eq!(archive.chats.len(), 1);
        assert_eq!(archive.chats[0].home, ChatHome::Gone, "its session expired");
    }

    #[test]
    fn not_at_all_empties_the_archive_and_always_keeps_it() {
        let mut archive = Archive::default();
        archive.sessions.push(session(1, GROUP, now() - days(400)));
        archive
            .chats
            .push(loose(8, GROUP, ChatHome::Gone, now() - days(400)));
        assert_eq!(archive.expire(ArchiveRetention::Always, now()), Vec::new());
        assert_eq!(archive.expire(ArchiveRetention::NotAtAll, now()).len(), 2);
        assert!(archive.is_empty());
    }

    #[test]
    fn sessions_list_newest_closed_first_within_their_group() {
        let mut archive = Archive::default();
        archive.sessions.push(session(1, GROUP, now() - days(5)));
        archive.sessions.push(session(2, OTHER, now()));
        archive.sessions.push(session(3, GROUP, now() - days(1)));
        let listed: Vec<u64> = archive
            .sessions_of(GROUP)
            .iter()
            .map(|session| session.id.as_u64())
            .collect();
        assert_eq!(listed, [3, 1]);
    }

    #[test]
    fn deleting_a_session_takes_its_loose_chats_with_it() {
        let mut archive = Archive::default();
        archive.sessions.push(session(1, GROUP, now()));
        archive.chats.push(loose(
            7,
            GROUP,
            ChatHome::Archived(ArchivedSessionId(1)),
            now(),
        ));
        archive.chats.push(loose(8, GROUP, ChatHome::Gone, now()));
        let deleted = archive.delete_session(ArchivedSessionId(1));
        assert_eq!(deleted.len(), 2);
        assert_eq!(archive.sessions.len(), 0);
        assert!(archive.holds(TerminalId(8)));
        assert!(!archive.holds(TerminalId(7)));
    }

    #[test]
    fn forgetting_a_group_keeps_the_other_groups() {
        let mut archive = Archive::default();
        archive.sessions.push(session(1, GROUP, now()));
        archive.sessions.push(session(2, OTHER, now()));
        archive.chats.push(loose(7, GROUP, ChatHome::Gone, now()));
        let deleted = archive.forget_group(GROUP);
        assert_eq!(deleted.len(), 2);
        assert_eq!(archive.sessions.len(), 1);
        assert_eq!(archive.chats.len(), 0);
        assert!(archive.holds(TerminalId(20)));
    }

    #[test]
    fn a_session_leaving_unarchived_orphans_its_loose_chats() {
        let mut archive = Archive::default();
        archive
            .chats
            .push(loose(7, GROUP, ChatHome::Open(SessionId(3)), now()));
        archive.leave_session(SessionId(3));
        assert_eq!(archive.chats[0].home, ChatHome::Gone);
    }
}
