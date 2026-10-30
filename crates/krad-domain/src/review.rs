mod remote;

use std::fmt;
use std::str::FromStr;

pub use remote::{project_on_host, remote_project, resolve_ssh_alias, RemoteProject};

const PATH_SEPARATOR: char = '/';

macro_rules! numeric_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

numeric_id!(UserId);
numeric_id!(MergeRequestIid);
numeric_id!(NoteId);
numeric_id!(PipelineId);
numeric_id!(JobId);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ProjectPath(String);

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidProjectPath;

impl fmt::Display for InvalidProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("A project path looks like group/project")
    }
}

impl std::error::Error for InvalidProjectPath {}

impl FromStr for ProjectPath {
    type Err = InvalidProjectPath;

    fn from_str(path: &str) -> Result<Self, InvalidProjectPath> {
        let path = path.trim().trim_matches(PATH_SEPARATOR);
        if path.is_empty() || path.split(PATH_SEPARATOR).any(str::is_empty) {
            return Err(InvalidProjectPath);
        }
        Ok(Self(path.to_owned()))
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for ProjectPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CommitSha(pub String);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DiscussionId(pub String);

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub name: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MergeRequestState {
    Opened,
    Closed,
    Locked,
    Merged,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MergeStatus {
    Mergeable,
    Checking,
    Unchecked,
    Conflict,
    NeedRebase,
    NotApproved,
    DiscussionsNotResolved,
    DraftStatus,
    CiMustPass,
    CiStillRunning,
    Other,
}

#[derive(Clone, PartialEq, Debug)]
pub struct MergeRequest {
    pub iid: MergeRequestIid,
    pub title: String,
    pub description: Option<String>,
    pub state: MergeRequestState,
    pub draft: bool,
    pub author: User,
    pub reviewers: Vec<User>,
    pub source_branch: String,
    pub target_branch: String,
    pub sha: Option<CommitSha>,
    pub detailed_merge_status: Option<MergeStatus>,
    pub has_conflicts: bool,
    pub user_notes_count: u32,
    pub web_url: String,
    pub created_at: String,
    pub updated_at: String,
    pub head_pipeline: Option<Pipeline>,
    pub diff_refs: Option<DiffRefs>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DiffRefs {
    pub base_sha: CommitSha,
    pub head_sha: CommitSha,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Commit {
    pub id: CommitSha,
    pub short_id: String,
    pub title: String,
    pub message: String,
    pub author_name: String,
    pub authored_date: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Change {
    pub old_path: String,
    pub new_path: String,
    pub diff: String,
    pub new_file: bool,
    pub renamed_file: bool,
    pub deleted_file: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Discussion {
    pub id: DiscussionId,
    pub individual_note: bool,
    pub notes: Vec<Note>,
}

impl Discussion {
    #[must_use]
    pub fn is_resolvable(&self) -> bool {
        self.notes.iter().any(|note| note.resolvable)
    }

    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.is_resolvable()
            && self
                .notes
                .iter()
                .filter(|note| note.resolvable)
                .all(|note| note.resolved)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Note {
    pub id: NoteId,
    pub body: String,
    pub author: User,
    pub created_at: String,
    pub system: bool,
    pub resolvable: bool,
    pub resolved: bool,
    pub position: Option<NotePosition>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NotePosition {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Approvals {
    pub approved: bool,
    pub approvals_required: u32,
    pub approvals_left: u32,
    pub approved_by: Vec<Approval>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Approval {
    pub user: User,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CiStatus {
    Created,
    WaitingForResource,
    Preparing,
    Pending,
    Running,
    Success,
    Failed,
    Canceling,
    Canceled,
    Skipped,
    Manual,
    Scheduled,
    Other,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pipeline {
    pub id: PipelineId,
    pub sha: CommitSha,
    pub git_ref: String,
    pub status: CiStatus,
    pub web_url: String,
    pub created_at: Option<String>,
    pub duration: Option<u64>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Job {
    pub id: JobId,
    pub name: String,
    pub stage: String,
    pub status: CiStatus,
    pub web_url: String,
    pub duration: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum StateFilter {
    #[default]
    Opened,
    Merged,
    Closed,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Scope {
    #[default]
    All,
    CreatedByMe,
    AssignedToMe,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct MergeRequestQuery {
    pub state: StateFilter,
    pub scope: Scope,
    pub reviewer: Option<UserId>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MergeOptions {
    pub sha: Option<CommitSha>,
    pub squash: bool,
    pub should_remove_source_branch: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_paths_reject_empty_segments() {
        assert_eq!(
            "/group/project/"
                .parse::<ProjectPath>()
                .map(|path| path.to_string()),
            Ok("group/project".to_owned())
        );
        assert_eq!(
            "group//project".parse::<ProjectPath>(),
            Err(InvalidProjectPath)
        );
        assert_eq!(" ".parse::<ProjectPath>(), Err(InvalidProjectPath));
    }
}
