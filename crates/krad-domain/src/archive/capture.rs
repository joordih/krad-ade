use super::{ArchivedChat, ArchivedSession, ArchivedSessionId, ArchivedShell, ChatHome, LooseChat};
use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
use crate::group::{OpenFile, Session, SessionLead, WorkerReopen};
use crate::ids::{SessionId, TerminalId};
use std::path::PathBuf;
use std::time::SystemTime;

fn archived_chat(node: &TerminalNode) -> Option<ArchivedChat> {
    if node.kind != NodeKind::Chat {
        return None;
    }
    Some(ArchivedChat {
        terminal: node.id,
        title: node.title.clone(),
        harness: node.harness.filter(|harness| harness.is_agent())?,
        resume: node.resume.clone()?,
        choice: node.choice.clone(),
        bookmarks: node.bookmarks.clone(),
        checkpoints: node.checkpoints.clone(),
    })
}

fn tiles(session: &Session) -> impl Iterator<Item = TerminalId> + '_ {
    session
        .tabs
        .tabs
        .iter()
        .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
}

struct Contents {
    chats: Vec<ArchivedChat>,
    shells: Vec<ArchivedShell>,
    files: Vec<OpenFile>,
}

impl EnvironmentData {
    fn node(&self, id: TerminalId) -> Option<&TerminalNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    fn is_worker_session(&self, id: SessionId) -> bool {
        self.sessions
            .iter()
            .filter_map(|session| session.lead.as_ref())
            .flat_map(|lead| &lead.workers)
            .any(|worker| worker.session == Some(id))
    }

    fn shell_of(&self, node: &TerminalNode) -> ArchivedShell {
        let cwd = self
            .meta
            .get(&node.id)
            .map(|meta| meta.cwd.as_str())
            .filter(|cwd| !cwd.is_empty())
            .map(PathBuf::from);
        ArchivedShell {
            title: node.title.clone(),
            cwd,
        }
    }

    fn contents_of(&self, session: &Session) -> Contents {
        let mut contents = Contents {
            chats: Vec::new(),
            shells: Vec::new(),
            files: Vec::new(),
        };
        for node in tiles(session).filter_map(|id| self.node(id)) {
            match node.kind {
                NodeKind::Chat => contents.chats.extend(archived_chat(node)),
                NodeKind::Terminal => contents.shells.push(self.shell_of(node)),
                NodeKind::File => contents
                    .files
                    .extend(node.file.clone().filter(|file| file.pinned)),
                NodeKind::Extension => {}
            }
        }
        contents
    }

    fn lead_with_workers(&self, session: &Session) -> (Option<SessionLead>, Vec<ArchivedChat>) {
        let Some(mut lead) = session.lead.clone() else {
            return (None, Vec::new());
        };
        let mut workers = Vec::new();
        for worker in &mut lead.workers {
            if worker.session.take().is_none() {
                continue;
            }
            let Some(node) = self.node(worker.terminal) else {
                continue;
            };
            let Some(chat) = archived_chat(node) else {
                continue;
            };
            worker.reopen = Some(WorkerReopen {
                harness: chat.harness,
                group: node.group,
                resume: chat.resume.clone(),
            });
            workers.push(chat);
        }
        (Some(lead), workers)
    }
}

impl EnvironmentData {
    fn closed_session(&self, id: SessionId, now: SystemTime) -> Option<ArchivedSession> {
        let session = self.session(id)?;
        let group = session.group?;
        if self.is_worker_session(id) {
            return None;
        }
        let Contents {
            chats,
            shells,
            files,
        } = self.contents_of(session);
        let (lead, workers) = self.lead_with_workers(session);
        if chats.is_empty() && files.is_empty() && workers.is_empty() {
            return None;
        }
        Some(ArchivedSession {
            id: self.archive.next_session_id(),
            group,
            name: session.name.clone(),
            look: session.look,
            closed: now,
            chats,
            shells,
            files,
            lead,
            workers,
        })
    }

    fn closed_chat(&self, terminal: TerminalId, now: SystemTime) -> Option<LooseChat> {
        if self.worker(terminal).is_some() {
            return None;
        }
        let chat = self.node(terminal).and_then(archived_chat)?;
        let session = self.session(self.session_of(terminal)?)?;
        Some(LooseChat {
            group: session.group?,
            home: ChatHome::Open(session.id),
            session_name: session.name.clone(),
            closed: now,
            chat,
        })
    }

    pub fn archive_session(&mut self, id: SessionId, now: SystemTime) -> Option<ArchivedSessionId> {
        let Some(archived) = self.closed_session(id, now) else {
            self.archive.leave_session(id);
            return None;
        };
        let kept = archived.id;
        self.archive.sessions.push(archived);
        self.archive
            .rehome_chats(ChatHome::Open(id), ChatHome::Archived(kept));
        Some(kept)
    }

    pub fn archive_chat(&mut self, terminal: TerminalId, now: SystemTime) -> bool {
        let Some(loose) = self.closed_chat(terminal, now) else {
            return false;
        };
        self.archive.chats.push(loose);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{now, Workbench};
    use super::*;
    use crate::agent::AgentPreset;
    use crate::environment::TerminalMeta;
    use crate::group::{FileOpening, WorkerSpawn};
    use std::path::Path;

    #[test]
    fn a_closed_session_keeps_its_chats_shells_and_pinned_files() {
        let mut bench = Workbench::new();
        let chat = bench.push(NodeKind::Chat, Some("chat-1"));
        bench.push(NodeKind::Chat, None);
        let shell = bench.push(NodeKind::Terminal, None);
        bench.data.meta.insert(
            shell,
            TerminalMeta {
                cwd: "/work/api/src".to_owned(),
                scrollback: vec!["secret output".to_owned()],
            },
        );
        bench.data.open_file(
            Some(bench.group),
            Path::new("/work/api/a.rs"),
            FileOpening::Pinned,
        );
        bench.data.open_file(
            Some(bench.group),
            Path::new("/work/api/b.rs"),
            FileOpening::Preview,
        );
        let session = bench.current();
        let kept = bench.data.archive_session(session, now());
        assert!(kept.is_some());
        let archived = &bench.data.archive.sessions[0];
        assert_eq!(archived.group, bench.group);
        assert_eq!(
            archived
                .chats
                .iter()
                .map(|chat| chat.terminal)
                .collect::<Vec<_>>(),
            [chat],
            "a chat that never started has nothing to resume"
        );
        assert_eq!(
            archived.shells,
            [ArchivedShell {
                title: format!("Tab {}", shell.as_u64()),
                cwd: Some(PathBuf::from("/work/api/src")),
            }]
        );
        assert_eq!(archived.files.len(), 1, "only the pinned file reopens");
        assert_eq!(archived.files[0].path, Path::new("/work/api/a.rs"));
        assert!(bench.data.archive.holds(chat));
    }

    #[test]
    fn a_session_with_nothing_to_reopen_is_not_kept() {
        let mut bench = Workbench::new();
        bench.push(NodeKind::Chat, None);
        bench.push(NodeKind::Terminal, None);
        let session = bench.current();
        assert!(bench.data.archive_session(session, now()).is_none());
        assert!(bench.data.archive.is_empty());
    }

    #[test]
    fn a_lead_is_archived_with_its_open_workers_ready_to_reopen() {
        let mut bench = Workbench::new();
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
        let worker_session = bench.data.open_worker_session(
            worker,
            WorkerSpawn {
                lead: Some(lead),
                task: "Write the tests".to_owned(),
            },
        );
        bench.data.active_session = Some(lead);
        assert!(worker_session.is_some_and(|id| bench.data.archive_session(id, now()).is_none()));
        let kept = bench.data.archive_session(lead, now());
        assert!(kept.is_some());
        let archived = &bench.data.archive.sessions[0];
        assert_eq!(archived.chat_count(), 2);
        assert_eq!(archived.workers[0].terminal, worker);
        let workers = archived
            .lead
            .as_ref()
            .map_or_default(|lead| lead.workers.clone());
        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].session, None);
        assert_eq!(
            workers[0]
                .reopen
                .as_ref()
                .map(|reopen| reopen.resume.as_str()),
            Some("worker-1")
        );
    }

    #[test]
    fn a_closed_chat_tab_hangs_from_its_open_session() {
        let mut bench = Workbench::new();
        let chat = bench.push(NodeKind::Chat, Some("chat-1"));
        assert!(bench.data.archive_chat(chat, now()));
        assert_eq!(
            bench.data.archive.chats[0].home,
            ChatHome::Open(bench.current())
        );
        let session = bench.current();
        let kept = bench.data.archive_session(session, now());
        assert_eq!(
            kept.map(ChatHome::Archived),
            Some(bench.data.archive.chats[0].home)
        );
    }
}
