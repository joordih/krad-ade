mod capture;
mod reopen;
mod retention;
mod shelf;

pub use reopen::*;
pub use retention::*;

use crate::agent::AgentPreset;
use crate::bookmark::ChatBookmarks;
use crate::checkpoint::ChatCheckpoints;
use crate::group::{OpenFile, SessionLead, SessionLook};
use crate::ids::{GroupId, ResumeId, SessionId, TerminalId};
use crate::model::ChatChoice;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArchivedSessionId(pub u64);

impl ArchivedSessionId {
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedChat {
    pub terminal: TerminalId,
    pub title: String,
    pub harness: AgentPreset,
    pub resume: ResumeId,
    #[serde(default, skip_serializing_if = "ChatChoice::is_empty")]
    pub choice: ChatChoice,
    #[serde(default, skip_serializing_if = "ChatBookmarks::is_empty")]
    pub bookmarks: ChatBookmarks,
    #[serde(default, skip_serializing_if = "ChatCheckpoints::is_empty")]
    pub checkpoints: ChatCheckpoints,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedShell {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedSession {
    pub id: ArchivedSessionId,
    pub group: GroupId,
    pub name: String,
    #[serde(default, skip_serializing_if = "SessionLook::is_plain")]
    pub look: SessionLook,
    pub closed: SystemTime,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chats: Vec<ArchivedChat>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shells: Vec<ArchivedShell>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<OpenFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<SessionLead>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workers: Vec<ArchivedChat>,
}

impl ArchivedSession {
    #[must_use]
    pub fn chat_count(&self) -> usize {
        self.chats.len() + self.workers.len()
    }

    pub fn all_chats(&self) -> impl Iterator<Item = &ArchivedChat> {
        self.chats.iter().chain(&self.workers)
    }

    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let needle = query.trim().to_lowercase();
        needle.is_empty()
            || self.name.to_lowercase().contains(&needle)
            || self
                .all_chats()
                .any(|chat| chat.title.to_lowercase().contains(&needle))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatHome {
    Open(SessionId),
    Archived(ArchivedSessionId),
    Gone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LooseChat {
    pub group: GroupId,
    pub home: ChatHome,
    pub session_name: String,
    pub closed: SystemTime,
    pub chat: ArchivedChat,
}

impl LooseChat {
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let needle = query.trim().to_lowercase();
        needle.is_empty()
            || self.chat.title.to_lowercase().contains(&needle)
            || self.session_name.to_lowercase().contains(&needle)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Archive {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<ArchivedSession>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chats: Vec<LooseChat>,
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::agent::AgentState;
    use crate::environment::{EnvironmentData, NodeKind, TerminalNode};
    use crate::group::SessionSpec;
    use crate::ids::EnvironmentName;
    use crate::text_scale::TextScale;
    use crate::usage::ChatUsage;
    use std::time::Duration;

    pub(crate) const NOW_SINCE_EPOCH: Duration = Duration::from_hours(24 * 400);

    pub(crate) fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + NOW_SINCE_EPOCH
    }

    pub(crate) fn resume(raw: &str) -> ResumeId {
        raw.parse()
            .unwrap_or_else(|_| unreachable!("a valid resume id"))
    }

    pub(crate) fn chat(terminal: u64, title: &str) -> ArchivedChat {
        ArchivedChat {
            terminal: TerminalId(terminal),
            title: title.to_owned(),
            harness: AgentPreset::ClaudeCode,
            resume: resume(&format!("resume-{terminal}")),
            choice: ChatChoice::default(),
            bookmarks: ChatBookmarks::default(),
            checkpoints: ChatCheckpoints::default(),
        }
    }

    pub(crate) fn session(id: u64, group: GroupId, closed: SystemTime) -> ArchivedSession {
        ArchivedSession {
            id: ArchivedSessionId(id),
            group,
            name: format!("Session {id}"),
            look: SessionLook::default(),
            closed,
            chats: vec![chat(id * 10, "Fix the build")],
            shells: Vec::new(),
            files: Vec::new(),
            lead: None,
            workers: Vec::new(),
        }
    }

    pub(crate) struct Workbench {
        pub(crate) data: EnvironmentData,
        pub(crate) group: GroupId,
    }

    impl Workbench {
        pub(crate) fn new() -> Self {
            let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
            let group = data.add_group(PathBuf::from("/work/api"), None);
            data.enter_sessions();
            data.new_session(Some(group), SessionSpec::default());
            Self { data, group }
        }

        pub(crate) fn push(&mut self, kind: NodeKind, resume_id: Option<&str>) -> TerminalId {
            let id = TerminalId(self.data.next_id);
            self.data.next_id += 1;
            self.data.nodes.push(TerminalNode {
                id,
                title: format!("Tab {}", id.as_u64()),
                state: AgentState::Idle,
                harness: (kind == NodeKind::Chat).then_some(AgentPreset::ClaudeCode),
                resume: resume_id.map(resume),
                group: Some(self.group),
                text_scale: TextScale::default(),
                kind,
                file: None,
                extension: None,
                bookmarks: ChatBookmarks::default(),
                usage: ChatUsage::default(),
                checkpoints: ChatCheckpoints::default(),
                choice: ChatChoice::default(),
            });
            self.data.place_terminal(id, None);
            id
        }

        pub(crate) fn current(&self) -> SessionId {
            self.data
                .active_session
                .unwrap_or_else(|| unreachable!("a session is shown"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{chat, now, session};
    use super::*;

    #[test]
    fn an_archive_round_trips_with_its_sessions_and_loose_chats() -> serde_json::Result<()> {
        let mut archive = Archive::default();
        archive.sessions.push(session(1, GroupId(1), now()));
        archive.chats.push(LooseChat {
            group: GroupId(1),
            home: ChatHome::Open(SessionId(4)),
            session_name: "Main".to_owned(),
            closed: now(),
            chat: chat(9, "Review the plan"),
        });
        let back: Archive = serde_json::from_str(&serde_json::to_string(&archive)?)?;
        assert_eq!(back, archive);
        Ok(())
    }

    #[test]
    fn a_session_matches_its_name_or_any_chat_title_ignoring_case() {
        let archived = session(1, GroupId(1), now());
        assert!(archived.matches(""));
        assert!(archived.matches("session"));
        assert!(archived.matches("BUILD"));
        assert!(!archived.matches("deploy"));
    }
}
