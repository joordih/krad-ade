use krad_domain::review::{
    Change, CiStatus, Commit, CommitSha, Discussion, DiscussionId, Job, JobId, MergeRequest,
    MergeRequestIid, MergeRequestState, MergeStatus, Note, NoteId, Pipeline, PipelineId, User,
    UserId,
};
use serde::{Deserialize, Serialize};

const SHORT_SHA: usize = 7;
const TITLE_END: char = '\n';
pub(crate) const CONVERSATION_PREFIX: &str = "conversation:";

#[derive(Deserialize)]
pub(crate) struct UserJson {
    pub(crate) id: u64,
    login: String,
    #[serde(default)]
    name: Option<String>,
}

impl From<UserJson> for User {
    fn from(user: UserJson) -> Self {
        Self {
            id: UserId(user.id),
            name: user.name.unwrap_or_else(|| user.login.clone()),
            username: user.login,
        }
    }
}

#[derive(Deserialize)]
struct BranchRefJson {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PullState {
    Open,
    Closed,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MergeableState {
    Clean,
    Unstable,
    HasHooks,
    Dirty,
    Behind,
    Blocked,
    Draft,
    Unknown,
    #[serde(other)]
    Other,
}

impl From<MergeableState> for MergeStatus {
    fn from(state: MergeableState) -> Self {
        match state {
            MergeableState::Clean | MergeableState::Unstable | MergeableState::HasHooks => {
                Self::Mergeable
            }
            MergeableState::Dirty => Self::Conflict,
            MergeableState::Behind => Self::NeedRebase,
            MergeableState::Blocked => Self::NotApproved,
            MergeableState::Draft => Self::DraftStatus,
            MergeableState::Unknown => Self::Checking,
            MergeableState::Other => Self::Other,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PullJson {
    pub(crate) number: u64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    state: PullState,
    #[serde(default)]
    draft: bool,
    user: UserJson,
    #[serde(default)]
    pub(crate) requested_reviewers: Vec<UserJson>,
    #[serde(default)]
    pub(crate) assignees: Vec<UserJson>,
    head: BranchRefJson,
    base: BranchRefJson,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    mergeable: Option<bool>,
    #[serde(default)]
    mergeable_state: Option<MergeableState>,
    #[serde(default)]
    comments: u32,
    #[serde(default)]
    review_comments: u32,
    html_url: String,
    created_at: String,
    updated_at: String,
}

impl PullJson {
    pub(crate) fn is_merged(&self) -> bool {
        self.merged_at.is_some()
    }

    pub(crate) fn author_id(&self) -> UserId {
        UserId(self.user.id)
    }

    pub(crate) fn head_sha(&self) -> &str {
        &self.head.sha
    }

    pub(crate) fn branches(&self) -> (&str, &str) {
        (&self.base.name, &self.head.name)
    }
}

impl From<PullJson> for MergeRequest {
    fn from(pull: PullJson) -> Self {
        let state = match (pull.state, pull.merged_at.is_some()) {
            (_, true) => MergeRequestState::Merged,
            (PullState::Open, false) => MergeRequestState::Opened,
            (PullState::Closed, false) => MergeRequestState::Closed,
            (PullState::Other, false) => MergeRequestState::Other,
        };
        let status = pull.mergeable_state.map(MergeStatus::from);
        Self {
            iid: MergeRequestIid(pull.number),
            title: pull.title,
            description: pull.body,
            state,
            draft: pull.draft,
            author: pull.user.into(),
            reviewers: pull
                .requested_reviewers
                .into_iter()
                .map(User::from)
                .collect(),
            source_branch: pull.head.name,
            target_branch: pull.base.name,
            sha: Some(CommitSha(pull.head.sha)),
            detailed_merge_status: status,
            has_conflicts: status == Some(MergeStatus::Conflict) || pull.mergeable == Some(false),
            user_notes_count: pull.comments + pull.review_comments,
            web_url: pull.html_url,
            created_at: pull.created_at,
            updated_at: pull.updated_at,
            head_pipeline: None,
            diff_refs: None,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct CompareJson {
    pub(crate) merge_base_commit: ShaJson,
}

#[derive(Deserialize)]
pub(crate) struct ShaJson {
    pub(crate) sha: String,
}

#[derive(Deserialize)]
struct CommitAuthorJson {
    name: String,
    #[serde(default)]
    date: Option<String>,
}

#[derive(Deserialize)]
struct CommitDetailJson {
    message: String,
    author: CommitAuthorJson,
}

#[derive(Deserialize)]
pub(crate) struct CommitJson {
    sha: String,
    commit: CommitDetailJson,
    #[serde(default)]
    html_url: Option<String>,
}

impl From<CommitJson> for Commit {
    fn from(commit: CommitJson) -> Self {
        let message = commit.commit.message;
        Self {
            short_id: commit.sha.chars().take(SHORT_SHA).collect(),
            title: message
                .split(TITLE_END)
                .next()
                .unwrap_or_default()
                .to_owned(),
            id: CommitSha(commit.sha),
            message,
            author_name: commit.commit.author.name,
            authored_date: commit.commit.author.date,
            web_url: commit.html_url,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileStatus {
    Added,
    Removed,
    Renamed,
    #[serde(other)]
    Changed,
}

#[derive(Deserialize)]
pub(crate) struct FileJson {
    filename: String,
    #[serde(default)]
    previous_filename: Option<String>,
    status: FileStatus,
    #[serde(default)]
    patch: Option<String>,
}

impl From<FileJson> for Change {
    fn from(file: FileJson) -> Self {
        Self {
            old_path: file
                .previous_filename
                .unwrap_or_else(|| file.filename.clone()),
            new_file: file.status == FileStatus::Added,
            renamed_file: file.status == FileStatus::Renamed,
            deleted_file: file.status == FileStatus::Removed,
            new_path: file.filename,
            diff: file.patch.unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct IssueCommentJson {
    id: u64,
    node_id: String,
    body: String,
    user: UserJson,
    created_at: String,
}

impl From<IssueCommentJson> for Note {
    fn from(comment: IssueCommentJson) -> Self {
        Self {
            id: NoteId(comment.id),
            body: comment.body,
            author: comment.user.into(),
            created_at: comment.created_at,
            system: false,
            resolvable: false,
            resolved: false,
            position: None,
        }
    }
}

impl From<IssueCommentJson> for Discussion {
    fn from(comment: IssueCommentJson) -> Self {
        Self {
            id: DiscussionId(format!("{CONVERSATION_PREFIX}{}", comment.node_id)),
            individual_note: true,
            notes: vec![comment.into()],
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ReviewJson {
    pub(crate) id: u64,
    node_id: String,
    pub(crate) user: Option<UserJson>,
    #[serde(default)]
    body: Option<String>,
    pub(crate) state: ReviewState,
    #[serde(default)]
    submitted_at: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ReviewState {
    Approved,
    ChangesRequested,
    Dismissed,
    #[serde(other)]
    Commented,
}

impl ReviewState {
    pub(crate) const fn is_verdict(self) -> bool {
        !matches!(self, Self::Commented)
    }
}

impl ReviewJson {
    pub(crate) fn into_discussion(self) -> Option<Discussion> {
        let body = self.body.filter(|body| !body.trim().is_empty())?;
        let author = self.user?;
        Some(Discussion {
            id: DiscussionId(format!("{CONVERSATION_PREFIX}{}", self.node_id)),
            individual_note: true,
            notes: vec![Note {
                id: NoteId(self.id),
                body,
                author: author.into(),
                created_at: self.submitted_at.unwrap_or_default(),
                system: false,
                resolvable: false,
                resolved: false,
                position: None,
            }],
        })
    }

    pub(crate) fn author(&self) -> Option<User> {
        self.user.as_ref().map(|user| User {
            id: UserId(user.id),
            username: user.login.clone(),
            name: user.name.clone().unwrap_or_else(|| user.login.clone()),
        })
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RunStatus {
    Completed,
    InProgress,
    Queued,
    Requested,
    Pending,
    Waiting,
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Conclusion {
    Success,
    Failure,
    TimedOut,
    StartupFailure,
    Cancelled,
    Skipped,
    Neutral,
    Stale,
    ActionRequired,
    #[serde(other)]
    Other,
}

fn ci_status(status: RunStatus, conclusion: Option<Conclusion>) -> CiStatus {
    match (status, conclusion) {
        (RunStatus::Completed, Some(Conclusion::Success)) => CiStatus::Success,
        (
            RunStatus::Completed,
            Some(Conclusion::Failure | Conclusion::TimedOut | Conclusion::StartupFailure),
        ) => CiStatus::Failed,
        (RunStatus::Completed, Some(Conclusion::Cancelled)) => CiStatus::Canceled,
        (
            RunStatus::Completed,
            Some(Conclusion::Skipped | Conclusion::Neutral | Conclusion::Stale),
        ) => CiStatus::Skipped,
        (RunStatus::Completed, Some(Conclusion::ActionRequired)) => CiStatus::Manual,
        (RunStatus::InProgress, _) => CiStatus::Running,
        (RunStatus::Queued | RunStatus::Requested | RunStatus::Pending, _) => CiStatus::Pending,
        (RunStatus::Waiting, _) => CiStatus::WaitingForResource,
        (RunStatus::Completed | RunStatus::Other, _) => CiStatus::Other,
    }
}

#[derive(Deserialize)]
pub(crate) struct RunsJson {
    pub(crate) workflow_runs: Vec<RunJson>,
}

#[derive(Deserialize)]
pub(crate) struct RunJson {
    id: u64,
    head_sha: String,
    #[serde(default)]
    head_branch: Option<String>,
    status: RunStatus,
    #[serde(default)]
    conclusion: Option<Conclusion>,
    html_url: String,
    #[serde(default)]
    created_at: Option<String>,
}

impl From<RunJson> for Pipeline {
    fn from(run: RunJson) -> Self {
        Self {
            id: PipelineId(run.id),
            sha: CommitSha(run.head_sha),
            git_ref: run.head_branch.unwrap_or_default(),
            status: ci_status(run.status, run.conclusion),
            web_url: run.html_url,
            created_at: run.created_at,
            duration: None,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct JobsJson {
    pub(crate) jobs: Vec<JobJson>,
}

#[derive(Deserialize)]
pub(crate) struct JobJson {
    id: u64,
    name: String,
    #[serde(default)]
    workflow_name: Option<String>,
    status: RunStatus,
    #[serde(default)]
    conclusion: Option<Conclusion>,
    html_url: String,
}

impl From<JobJson> for Job {
    fn from(job: JobJson) -> Self {
        Self {
            id: JobId(job.id),
            name: job.name,
            stage: job.workflow_name.unwrap_or_default(),
            status: ci_status(job.status, job.conclusion),
            web_url: job.html_url,
            duration: None,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct CommentBody<'a> {
    pub(crate) body: &'a str,
}

#[derive(Serialize)]
pub(crate) struct ReviewBody<'a> {
    pub(crate) event: &'a str,
}

#[derive(Serialize)]
pub(crate) struct DismissBody<'a> {
    pub(crate) message: &'a str,
    pub(crate) event: &'a str,
}

#[derive(Serialize)]
pub(crate) struct MergeBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sha: Option<&'a str>,
    pub(crate) merge_method: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    const PULL: &str = r#"{
        "number": 7, "title": "Rotate tokens", "body": null, "state": "closed",
        "draft": false, "user": {"id": 1, "login": "ana"},
        "requested_reviewers": [{"id": 2, "login": "bo"}],
        "head": {"ref": "rotate", "sha": "head111"}, "base": {"ref": "main", "sha": "base000"},
        "merged_at": "2026-10-01T10:00:00Z", "mergeable_state": "dirty",
        "comments": 2, "review_comments": 3,
        "html_url": "https://github.com/o/r/pull/7",
        "created_at": "2026-09-30T10:00:00Z", "updated_at": "2026-10-01T10:00:00Z"
    }"#;

    #[test]
    fn a_pull_request_reads_as_a_merge_request() -> serde_json::Result<()> {
        let request = MergeRequest::from(serde_json::from_str::<PullJson>(PULL)?);
        assert_eq!(request.iid, MergeRequestIid(7));
        assert_eq!(request.state, MergeRequestState::Merged);
        assert_eq!(
            request.author.name, "ana",
            "the login names a user without a name"
        );
        assert_eq!(request.reviewers[0].username, "bo");
        assert_eq!(request.detailed_merge_status, Some(MergeStatus::Conflict));
        assert!(request.has_conflicts);
        assert_eq!(request.user_notes_count, 5);
        assert_eq!(request.sha, Some(CommitSha("head111".to_owned())));
        Ok(())
    }

    #[test]
    fn a_commit_takes_its_title_from_the_first_line() -> serde_json::Result<()> {
        let commit = Commit::from(serde_json::from_str::<CommitJson>(
            r#"{"sha":"0123456789abcdef","commit":{"message":"Fix race\n\nLong text","author":{"name":"Ana","date":"2026-10-01T10:00:00Z"}}}"#,
        )?);
        assert_eq!(commit.short_id, "0123456");
        assert_eq!(commit.title, "Fix race");
        Ok(())
    }

    #[test]
    fn a_renamed_file_keeps_its_old_path() -> serde_json::Result<()> {
        let change = Change::from(serde_json::from_str::<FileJson>(
            r#"{"filename":"src/new.rs","previous_filename":"src/old.rs","status":"renamed","patch":"@@ -1 +1 @@"}"#,
        )?);
        assert_eq!(
            (change.old_path.as_str(), change.new_path.as_str()),
            ("src/old.rs", "src/new.rs")
        );
        assert!(change.renamed_file && !change.new_file);
        Ok(())
    }

    #[test]
    fn runs_read_their_status_and_conclusion_as_one() {
        let run = |json: &str| -> serde_json::Result<CiStatus> {
            serde_json::from_str::<RunJson>(json).map(|run| Pipeline::from(run).status)
        };
        let base = r#""id":1,"head_sha":"a","html_url":"u""#;
        assert_eq!(
            run(&format!(
                r#"{{{base},"status":"completed","conclusion":"success"}}"#
            ))
            .ok(),
            Some(CiStatus::Success)
        );
        assert_eq!(
            run(&format!(
                r#"{{{base},"status":"completed","conclusion":"timed_out"}}"#
            ))
            .ok(),
            Some(CiStatus::Failed)
        );
        assert_eq!(
            run(&format!(r#"{{{base},"status":"in_progress"}}"#)).ok(),
            Some(CiStatus::Running)
        );
        assert_eq!(
            run(&format!(r#"{{{base},"status":"queued"}}"#)).ok(),
            Some(CiStatus::Pending)
        );
    }

    #[test]
    fn a_review_without_a_summary_is_no_discussion() -> serde_json::Result<()> {
        let review: ReviewJson = serde_json::from_str(
            r#"{"id":3,"node_id":"PRR_x","user":{"id":2,"login":"bo"},"body":"","state":"APPROVED"}"#,
        )?;
        assert!(review.into_discussion().is_none());
        let summary: ReviewJson = serde_json::from_str(
            r#"{"id":4,"node_id":"PRR_y","user":{"id":2,"login":"bo"},"body":"Looks good","state":"COMMENTED"}"#,
        )?;
        let discussion = summary.into_discussion();
        assert_eq!(
            discussion.map(|discussion| discussion.id),
            Some(DiscussionId("conversation:PRR_y".to_owned()))
        );
        Ok(())
    }
}
