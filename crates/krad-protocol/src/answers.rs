use krad_domain::agents_api::WorktreeOutcome;
use krad_domain::{
    AcpAgentId, AcpPackageState, CheckpointChange, CommitFile, HistoryCommit, IndexEntry,
    LibrarySkill, ProjectTask, RestoredCheckpoint, SourceEntry, StoredTemplate, Worktree,
    WorktreeState,
};
use krad_domain::{
    DirEntry, FileStat, ImageFormat, ProjectMatches, ReplacedFile, Skill, TextMatch, UndoOutcome,
    VaultEntry,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::chats::{ChatPage, HistoryPart, SubagentsRead, TranscriptRead};
use crate::command::ErrorCode;
use crate::files::{TextRead, Unreadable, VaultNoteText};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    ChatPage(ChatPage),
    Transcript(Box<TranscriptRead>),
    Subagents(Box<SubagentsRead>),
    History(Box<HistoryPart>),
    Mentions(FoundMentions),
    Done,
    DocsSources {
        picked: Option<Vec<String>>,
        candidates: Vec<SourceEntry>,
    },
    Templates {
        templates: Vec<StoredTemplate>,
    },
    WorktreeAdded {
        worktree: Worktree,
    },
    WorktreeStates {
        states: Vec<WorktreeChecked>,
    },
    SettledRemoval {
        outcome: WorktreeOutcome,
    },
    RemoteUrls {
        urls: Vec<Result<Option<String>, String>>,
    },
    Commits {
        commits: Vec<HistoryCommit>,
    },
    CommitFiles {
        files: Vec<CommitFile>,
    },
    Diff {
        text: String,
    },
    Diffs {
        texts: Vec<String>,
    },
    Tasks {
        tasks: Vec<ProjectTask>,
    },
    NoteAdded {
        name: String,
    },
    CheckpointChanges {
        changes: Vec<CheckpointChange>,
    },
    CheckpointRestored {
        restored: RestoredCheckpoint,
    },
    Text {
        text: String,
        truncated: bool,
    },
    TextReplaced {
        replaced: bool,
    },
    Skills {
        skills: Vec<LibrarySkill>,
    },
    ChatSkills {
        skills: Vec<Skill>,
    },
    SshHosts {
        aliases: Vec<String>,
    },
    Folders {
        dir: PathBuf,
        names: Option<Vec<String>>,
    },
    AcpPackages {
        packages: Vec<AcpPackageShown>,
    },
    FileText {
        text: String,
        len: u64,
        truncated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modified: Option<SystemTime>,
    },
    Unreadable {
        unreadable: Unreadable,
    },
    ImageBytes {
        format: ImageFormat,
        bytes: Vec<u8>,
    },
    Stat {
        stat: Option<FileStat>,
    },
    Written {
        stat: Option<FileStat>,
    },
    Entries {
        entries: Vec<DirEntry>,
    },
    Placed {
        path: PathBuf,
    },
    Taken {
        names: Vec<String>,
    },
    Present {
        present: Vec<bool>,
    },
    Texts {
        texts: Vec<TextRead>,
    },
    Indexed {
        files: Vec<PathBuf>,
    },
    ProjectMatches {
        matches: ProjectMatches,
    },
    Replaced {
        files: Vec<Result<ReplacedFile, String>>,
    },
    Undone {
        outcomes: Vec<Result<UndoOutcome, String>>,
    },
    TextMatches {
        matches: Vec<TextMatch>,
    },
    Vault {
        entries: Vec<VaultEntry>,
        notes: Vec<VaultNoteText>,
    },
    Failed {
        code: ErrorCode,
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct FoundMentions {
    pub entries: Vec<IndexEntry>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorktreeChecked {
    pub path: PathBuf,
    pub state: Result<WorktreeState, String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AcpPackageShown {
    pub agent: AcpAgentId,
    pub state: AcpPackageState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::AgentRef;
    use crate::event::EventKind;
    use crate::ids::RequestId;
    use krad_domain::{EntryKind, EnvironmentName, TerminalId};
    use std::path::PathBuf;

    #[test]
    fn an_answer_names_its_request_and_kind_on_the_wire() -> serde_json::Result<()> {
        let answered = EventKind::Answered {
            request: RequestId(7),
            answer: Box::new(Answer::Mentions(FoundMentions {
                entries: vec![IndexEntry {
                    path: PathBuf::from("src/main.rs"),
                    kind: EntryKind::File,
                }],
            })),
        };
        let json = serde_json::to_value(&answered)?;
        assert_eq!(json["answer"]["kind"], "mentions");
        assert_eq!(json["answer"]["entries"][0]["kind"], "file");
        assert_eq!(serde_json::from_value::<EventKind>(json)?, answered);
        let page = Answer::ChatPage(ChatPage {
            chat: AgentRef {
                environment: EnvironmentName::new("main".to_owned()),
                terminal: TerminalId(2),
            },
            len: 0,
            from: 0,
            messages: Vec::new(),
        });
        let json = serde_json::to_value(&page)?;
        assert_eq!(json["kind"], "chat_page");
        assert_eq!(serde_json::from_value::<Answer>(json)?, page);
        let newer = serde_json::json!({ "kind": "galaxy", "stars": 3 });
        assert_eq!(serde_json::from_value::<Answer>(newer)?, Answer::Unknown);
        Ok(())
    }
}
