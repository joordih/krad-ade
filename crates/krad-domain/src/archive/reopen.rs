use super::{ArchivedChat, ArchivedSession, ArchivedSessionId, ArchivedShell, ChatHome, LooseChat};
use crate::agent::AgentState;
use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
use crate::group::{FileOpening, SessionSpec};
use crate::ids::{GroupId, ResumeId, SessionId, TerminalId};
use crate::text_scale::TextScale;
use crate::usage::ChatUsage;
use crate::view::EnvironmentView;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumedChat {
    pub terminal: TerminalId,
    pub resume: ResumeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reopened {
    pub session: SessionId,
    pub chats: Vec<ResumedChat>,
    pub shells: Vec<ArchivedShell>,
    pub workers: Vec<TerminalId>,
}

impl EnvironmentData {
    pub fn claim_terminal_id(&mut self, wanted: TerminalId) -> TerminalId {
        let taken = self.nodes.iter().any(|node| node.id == wanted);
        if !taken && wanted.as_u64() < self.next_id {
            return wanted;
        }
        if !taken {
            self.next_id = wanted.as_u64() + 1;
            return wanted;
        }
        let fresh = TerminalId(self.next_id);
        self.next_id += 1;
        fresh
    }

    fn restore_chat(&mut self, chat: ArchivedChat, group: GroupId) -> ResumedChat {
        let terminal = self.claim_terminal_id(chat.terminal);
        let resume = chat.resume;
        self.nodes.push(TerminalNode {
            id: terminal,
            title: chat.title,
            state: AgentState::Idle,
            harness: Some(chat.harness),
            resume: Some(resume.clone()),
            group: Some(group),
            text_scale: TextScale::default(),
            kind: NodeKind::Chat,
            file: None,
            extension: None,
            bookmarks: chat.bookmarks,
            usage: ChatUsage::default(),
            checkpoints: chat.checkpoints,
            choice: chat.choice,
            cloud: None,
        });
        self.place_terminal(terminal, None);
        ResumedChat { terminal, resume }
    }

    fn reopen_session(&mut self, archived: ArchivedSession) -> Option<Reopened> {
        let group = archived.group;
        self.group(group)?;
        let id = self.new_session(Some(group), SessionSpec::default());
        self.view = EnvironmentView::Sessions;
        let mut moved: Vec<(TerminalId, TerminalId)> = Vec::new();
        let mut chats = Vec::new();
        for chat in archived.chats {
            let was = chat.terminal;
            let resumed = self.restore_chat(chat, group);
            moved.push((was, resumed.terminal));
            chats.push(resumed);
        }
        for file in archived.files {
            self.open_file_as(Some(group), &file.path, file.view, FileOpening::Pinned);
        }
        let mut lead = archived.lead;
        let mut workers = Vec::new();
        for worker in archived.workers {
            let claimed = self.claim_terminal_id(worker.terminal);
            let owned = lead
                .iter_mut()
                .flat_map(|lead| lead.workers.iter_mut())
                .find(|owned| owned.terminal == worker.terminal);
            if let Some(owned) = owned {
                owned.terminal = claimed;
                workers.push(claimed);
            }
        }
        if let Some(lead) = &mut lead {
            lead.terminal = lead.terminal.and_then(|old| {
                moved
                    .iter()
                    .find(|(was, _)| *was == old)
                    .map(|(_, now)| *now)
            });
        }
        let first = chats.first().map(|chat| chat.terminal);
        if let Some(session) = self.sessions.iter_mut().find(|found| found.id == id) {
            session.name = archived.name;
            session.look = archived.look;
            session.lead = lead;
            session.last_chat = first;
            session.cloud = archived.cloud;
        }
        self.active_session = Some(id);
        if let Some(first) = first {
            self.reveal_terminal(first);
        }
        Some(Reopened {
            session: id,
            chats,
            shells: archived.shells,
            workers,
        })
    }

    pub fn reopen_archived_session(&mut self, id: ArchivedSessionId) -> Option<Reopened> {
        let archived = self.archive.take_session(id)?;
        let reopened = self.reopen_session(archived)?;
        self.archive
            .rehome_chats(ChatHome::Archived(id), ChatHome::Open(reopened.session));
        Some(reopened)
    }

    pub fn reopen_archived_chat(&mut self, terminal: TerminalId) -> Option<ResumedChat> {
        let loose = self.archive.take_chat(terminal)?;
        self.reopen_chat(loose)
    }

    fn reopen_chat(&mut self, loose: LooseChat) -> Option<ResumedChat> {
        let group = loose.group;
        self.group(group)?;
        let home = match loose.home {
            ChatHome::Open(session) => self
                .session(session)
                .filter(|found| found.group == Some(group))
                .map(|found| found.id),
            ChatHome::Archived(_) | ChatHome::Gone => None,
        };
        let target = home
            .or_else(|| {
                self.current_session()
                    .filter(|found| found.group == Some(group))
                    .map(|found| found.id)
            })
            .or_else(|| self.sessions_of(Some(group)).next().map(|found| found.id))
            .unwrap_or_else(|| self.new_session(Some(group), SessionSpec::default()));
        self.active_session = Some(target);
        self.view = EnvironmentView::Sessions;
        let resumed = self.restore_chat(loose.chat, group);
        self.reveal_terminal(resumed.terminal);
        Some(resumed)
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{now, Workbench};
    use super::*;
    use crate::agent::AgentPreset;
    use crate::cloud::{CloudPlacement, CloudRunnerId};
    use crate::group::{SessionLead, SessionLook, WorkerSpawn};
    use crate::hue::Hue;
    use std::path::{Path, PathBuf};

    fn tab_ids(data: &EnvironmentData, session: SessionId) -> Vec<TerminalId> {
        data.session(session).map_or_default(|session| {
            session
                .tabs
                .tabs
                .iter()
                .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
                .collect()
        })
    }

    fn close_into(bench: &mut Workbench, session: SessionId) {
        let kept = bench.data.archive_session(session, now());
        assert!(kept.is_some());
        for terminal in tab_ids(&bench.data, session) {
            bench.data.drop_terminal(terminal);
        }
        bench.data.close_session(session);
    }

    fn lead_with_worker(bench: &mut Workbench) -> (SessionId, TerminalId, TerminalId) {
        let lead_chat = bench.push(NodeKind::Chat, Some("lead-1"));
        let lead = bench.current();
        if let Some(session) = bench
            .data
            .sessions
            .iter_mut()
            .find(|found| found.id == lead)
        {
            let mut chosen = SessionLead::new(AgentPreset::ClaudeCode, None);
            chosen.terminal = Some(lead_chat);
            session.lead = Some(chosen);
        }
        let worker = bench.push(NodeKind::Chat, Some("worker-1"));
        bench.data.open_worker_session(
            worker,
            WorkerSpawn {
                lead: Some(lead),
                task: "Write the tests".to_owned(),
            },
        );
        bench.data.active_session = Some(lead);
        (lead, lead_chat, worker)
    }

    #[test]
    fn reopening_a_session_brings_back_its_chats_and_file_tabs() {
        let mut bench = Workbench::new();
        let chat = bench.push(NodeKind::Chat, Some("chat-1"));
        bench.push(NodeKind::Terminal, None);
        let file = bench.data.open_file(
            Some(bench.group),
            Path::new("/work/api/a.rs"),
            FileOpening::Pinned,
        );
        let session = bench.current();
        if let Some(found) = bench
            .data
            .sessions
            .iter_mut()
            .find(|found| found.id == session)
        {
            found.name = "Refunds".to_owned();
            found.look = SessionLook {
                color: Some(Hue::Teal),
                icon: None,
            };
        }
        close_into(&mut bench, session);
        assert!(bench
            .data
            .nodes
            .iter()
            .all(|node| node.id != chat && node.id != file));
        let first = bench.data.archive.sessions[0].id;
        let Some(archived) = bench.data.archive.take_session(first) else {
            unreachable!("the session was archived");
        };
        let Some(reopened) = bench.data.reopen_session(archived) else {
            unreachable!("its group is still there");
        };
        assert_eq!(bench.data.active_session, Some(reopened.session));
        assert_eq!(reopened.chats.len(), 1);
        assert_eq!(reopened.chats[0].terminal, chat, "the chat keeps its id");
        assert_eq!(reopened.chats[0].resume.as_str(), "chat-1");
        assert_eq!(reopened.shells.len(), 1, "the shell starts afresh");
        let tabs = tab_ids(&bench.data, reopened.session);
        assert!(tabs.contains(&chat));
        let files: Vec<&Path> = tabs
            .iter()
            .filter_map(|id| bench.data.open_file_of(*id))
            .map(|file| file.path.as_path())
            .collect();
        assert_eq!(files, [Path::new("/work/api/a.rs")]);
        let shown = bench.data.session(reopened.session);
        assert_eq!(shown.map(|found| found.name.as_str()), Some("Refunds"));
        assert_eq!(shown.and_then(|found| found.look.color), Some(Hue::Teal));
    }

    #[test]
    fn a_cloud_session_reopens_on_its_runner_and_worktree() {
        let mut bench = Workbench::new();
        bench.push(NodeKind::Chat, Some("chat-1"));
        let session = bench.current();
        let placement = CloudPlacement {
            runner: CloudRunnerId(3),
            repo_dir: PathBuf::from("/home/me/.krad-ade/runner/repos/api-1a2b3c4d"),
            worktree: PathBuf::from("/home/me/.krad-ade/runner/worktrees/api-1a2b3c4d/api-77"),
            branch: "krad/api-77".to_owned(),
        };
        if let Some(found) = bench
            .data
            .sessions
            .iter_mut()
            .find(|found| found.id == session)
        {
            found.cloud = Some(placement.clone());
        }
        close_into(&mut bench, session);
        let first = bench.data.archive.sessions[0].id;
        let Some(archived) = bench.data.archive.take_session(first) else {
            unreachable!("the session was archived");
        };
        let Some(reopened) = bench.data.reopen_session(archived) else {
            unreachable!("its group is still there");
        };
        let shown = bench.data.session(reopened.session);
        assert_eq!(
            shown.and_then(|found| found.cloud.as_ref()),
            Some(&placement)
        );
    }

    #[test]
    fn a_lead_reopens_with_its_workers_owned_again() {
        let mut bench = Workbench::new();
        let (lead, lead_chat, worker) = lead_with_worker(&mut bench);
        assert!(bench.data.archive_session(lead, now()).is_some());
        bench.data.drop_terminal(worker);
        bench.data.drop_terminal(lead_chat);
        bench.data.close_session(lead);
        let first = bench.data.archive.sessions[0].id;
        let Some(archived) = bench.data.archive.take_session(first) else {
            unreachable!("the lead was archived");
        };
        let Some(reopened) = bench.data.reopen_session(archived) else {
            unreachable!("its group is still there");
        };
        assert_eq!(reopened.workers, [worker]);
        let owned = bench.data.lead(reopened.session);
        assert_eq!(owned.and_then(|lead| lead.terminal), Some(lead_chat));
        let card = owned.and_then(|lead| lead.workers.first());
        assert_eq!(card.map(|card| card.terminal), Some(worker));
        assert!(card.is_some_and(|card| card.session.is_none() && card.reopen.is_some()));
    }

    #[test]
    fn a_loose_chat_reopens_in_its_session_or_else_in_the_shown_one() {
        let mut bench = Workbench::new();
        let home = bench.current();
        let chat = bench.push(NodeKind::Chat, Some("chat-1"));
        assert!(bench.data.archive_chat(chat, now()));
        bench.data.drop_terminal(chat);
        let other = bench
            .data
            .new_session(Some(bench.group), SessionSpec::default());
        let Some(loose) = bench.data.archive.take_chat(chat) else {
            unreachable!("the chat was archived");
        };
        let mut orphan = loose.clone();
        let resumed = bench.data.reopen_chat(loose);
        assert_eq!(resumed.map(|found| found.terminal), Some(chat));
        assert_eq!(bench.data.session_of(chat), Some(home));
        bench.data.drop_terminal(chat);
        bench.data.active_session = Some(other);
        orphan.home = ChatHome::Gone;
        assert!(bench.data.reopen_chat(orphan).is_some());
        assert_eq!(bench.data.session_of(chat), Some(other));
    }

    #[test]
    fn a_taken_id_is_replaced_by_a_fresh_one() {
        let mut bench = Workbench::new();
        let open = bench.push(NodeKind::Terminal, None);
        let next = bench.data.next_id;
        assert_eq!(bench.data.claim_terminal_id(open), TerminalId(next));
        assert_eq!(bench.data.next_id, next + 1);
        let ahead = TerminalId(next + 10);
        assert_eq!(bench.data.claim_terminal_id(ahead), ahead);
        assert_eq!(bench.data.next_id, next + 11);
    }
}
