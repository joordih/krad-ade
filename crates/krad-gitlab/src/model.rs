use krad_domain::review::{
    Approval, Approvals, Change, CiStatus, Commit, CommitSha, DiffRefs, Discussion, DiscussionId,
    Job, JobId, MergeOptions, MergeRequest, MergeRequestIid, MergeRequestState, MergeStatus, Note,
    NoteId, NotePosition, Pipeline, PipelineId, Scope, StateFilter, User, UserId,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub(crate) struct UserJson {
    id: u64,
    username: String,
    name: String,
}

impl From<UserJson> for User {
    fn from(user: UserJson) -> Self {
        Self {
            id: UserId(user.id),
            username: user.username,
            name: user.name,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum StateJson {
    Opened,
    Closed,
    Locked,
    Merged,
    #[serde(other)]
    Other,
}

impl From<StateJson> for MergeRequestState {
    fn from(state: StateJson) -> Self {
        match state {
            StateJson::Opened => Self::Opened,
            StateJson::Closed => Self::Closed,
            StateJson::Locked => Self::Locked,
            StateJson::Merged => Self::Merged,
            StateJson::Other => Self::Other,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum MergeStatusJson {
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
    #[serde(other)]
    Other,
}

impl From<MergeStatusJson> for MergeStatus {
    fn from(status: MergeStatusJson) -> Self {
        match status {
            MergeStatusJson::Mergeable => Self::Mergeable,
            MergeStatusJson::Checking => Self::Checking,
            MergeStatusJson::Unchecked => Self::Unchecked,
            MergeStatusJson::Conflict => Self::Conflict,
            MergeStatusJson::NeedRebase => Self::NeedRebase,
            MergeStatusJson::NotApproved => Self::NotApproved,
            MergeStatusJson::DiscussionsNotResolved => Self::DiscussionsNotResolved,
            MergeStatusJson::DraftStatus => Self::DraftStatus,
            MergeStatusJson::CiMustPass => Self::CiMustPass,
            MergeStatusJson::CiStillRunning => Self::CiStillRunning,
            MergeStatusJson::Other => Self::Other,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct MergeRequestJson {
    iid: u64,
    title: String,
    #[serde(default)]
    description: Option<String>,
    state: StateJson,
    #[serde(default)]
    draft: bool,
    author: UserJson,
    #[serde(default)]
    reviewers: Vec<UserJson>,
    source_branch: String,
    target_branch: String,
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    detailed_merge_status: Option<MergeStatusJson>,
    #[serde(default)]
    has_conflicts: bool,
    #[serde(default)]
    user_notes_count: u32,
    web_url: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    head_pipeline: Option<PipelineJson>,
    #[serde(default)]
    diff_refs: Option<DiffRefsJson>,
}

impl From<MergeRequestJson> for MergeRequest {
    fn from(request: MergeRequestJson) -> Self {
        Self {
            iid: MergeRequestIid(request.iid),
            title: request.title,
            description: request.description,
            state: request.state.into(),
            draft: request.draft,
            author: request.author.into(),
            reviewers: request.reviewers.into_iter().map(User::from).collect(),
            source_branch: request.source_branch,
            target_branch: request.target_branch,
            sha: request.sha.map(CommitSha),
            detailed_merge_status: request.detailed_merge_status.map(MergeStatus::from),
            has_conflicts: request.has_conflicts,
            user_notes_count: request.user_notes_count,
            web_url: request.web_url,
            created_at: request.created_at,
            updated_at: request.updated_at,
            head_pipeline: request.head_pipeline.map(Pipeline::from),
            diff_refs: request.diff_refs.map(DiffRefs::from),
        }
    }
}

#[derive(Deserialize)]
struct DiffRefsJson {
    base_sha: String,
    head_sha: String,
}

impl From<DiffRefsJson> for DiffRefs {
    fn from(refs: DiffRefsJson) -> Self {
        Self {
            base_sha: CommitSha(refs.base_sha),
            head_sha: CommitSha(refs.head_sha),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct CommitJson {
    id: String,
    short_id: String,
    title: String,
    #[serde(default)]
    message: String,
    author_name: String,
    #[serde(default)]
    authored_date: Option<String>,
    #[serde(default)]
    web_url: Option<String>,
}

impl From<CommitJson> for Commit {
    fn from(commit: CommitJson) -> Self {
        Self {
            id: CommitSha(commit.id),
            short_id: commit.short_id,
            title: commit.title,
            message: commit.message,
            author_name: commit.author_name,
            authored_date: commit.authored_date,
            web_url: commit.web_url,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ChangeJson {
    old_path: String,
    new_path: String,
    diff: String,
    #[serde(default)]
    new_file: bool,
    #[serde(default)]
    renamed_file: bool,
    #[serde(default)]
    deleted_file: bool,
}

impl From<ChangeJson> for Change {
    fn from(change: ChangeJson) -> Self {
        Self {
            old_path: change.old_path,
            new_path: change.new_path,
            diff: change.diff,
            new_file: change.new_file,
            renamed_file: change.renamed_file,
            deleted_file: change.deleted_file,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct DiscussionJson {
    id: String,
    #[serde(default)]
    individual_note: bool,
    notes: Vec<NoteJson>,
}

impl From<DiscussionJson> for Discussion {
    fn from(discussion: DiscussionJson) -> Self {
        Self {
            id: DiscussionId(discussion.id),
            individual_note: discussion.individual_note,
            notes: discussion.notes.into_iter().map(Note::from).collect(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct NoteJson {
    id: u64,
    body: String,
    author: UserJson,
    created_at: String,
    #[serde(default)]
    system: bool,
    #[serde(default)]
    resolvable: bool,
    #[serde(default)]
    resolved: bool,
    #[serde(default)]
    position: Option<NotePositionJson>,
}

impl From<NoteJson> for Note {
    fn from(note: NoteJson) -> Self {
        Self {
            id: NoteId(note.id),
            body: note.body,
            author: note.author.into(),
            created_at: note.created_at,
            system: note.system,
            resolvable: note.resolvable,
            resolved: note.resolved,
            position: note.position.map(NotePosition::from),
        }
    }
}

#[derive(Deserialize)]
struct NotePositionJson {
    #[serde(default)]
    old_path: Option<String>,
    #[serde(default)]
    new_path: Option<String>,
    #[serde(default)]
    old_line: Option<u32>,
    #[serde(default)]
    new_line: Option<u32>,
}

impl From<NotePositionJson> for NotePosition {
    fn from(position: NotePositionJson) -> Self {
        Self {
            old_path: position.old_path,
            new_path: position.new_path,
            old_line: position.old_line,
            new_line: position.new_line,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ApprovalsJson {
    #[serde(default)]
    approved: bool,
    #[serde(default)]
    approvals_required: u32,
    #[serde(default)]
    approvals_left: u32,
    #[serde(default)]
    approved_by: Vec<ApprovalJson>,
}

#[derive(Deserialize)]
struct ApprovalJson {
    user: UserJson,
}

impl From<ApprovalsJson> for Approvals {
    fn from(approvals: ApprovalsJson) -> Self {
        Self {
            approved: approvals.approved,
            approvals_required: approvals.approvals_required,
            approvals_left: approvals.approvals_left,
            approved_by: approvals
                .approved_by
                .into_iter()
                .map(|approval| Approval {
                    user: approval.user.into(),
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CiStatusJson {
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
    #[serde(other)]
    Other,
}

impl From<CiStatusJson> for CiStatus {
    fn from(status: CiStatusJson) -> Self {
        match status {
            CiStatusJson::Created => Self::Created,
            CiStatusJson::WaitingForResource => Self::WaitingForResource,
            CiStatusJson::Preparing => Self::Preparing,
            CiStatusJson::Pending => Self::Pending,
            CiStatusJson::Running => Self::Running,
            CiStatusJson::Success => Self::Success,
            CiStatusJson::Failed => Self::Failed,
            CiStatusJson::Canceling => Self::Canceling,
            CiStatusJson::Canceled => Self::Canceled,
            CiStatusJson::Skipped => Self::Skipped,
            CiStatusJson::Manual => Self::Manual,
            CiStatusJson::Scheduled => Self::Scheduled,
            CiStatusJson::Other => Self::Other,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PipelineJson {
    id: u64,
    sha: String,
    #[serde(rename = "ref")]
    git_ref: String,
    status: CiStatusJson,
    web_url: String,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    duration: Option<u64>,
}

impl From<PipelineJson> for Pipeline {
    fn from(pipeline: PipelineJson) -> Self {
        Self {
            id: PipelineId(pipeline.id),
            sha: CommitSha(pipeline.sha),
            git_ref: pipeline.git_ref,
            status: pipeline.status.into(),
            web_url: pipeline.web_url,
            created_at: pipeline.created_at,
            duration: pipeline.duration,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct JobJson {
    id: u64,
    name: String,
    stage: String,
    status: CiStatusJson,
    web_url: String,
    #[serde(default)]
    duration: Option<f64>,
}

impl From<JobJson> for Job {
    fn from(job: JobJson) -> Self {
        Self {
            id: JobId(job.id),
            name: job.name,
            stage: job.stage,
            status: job.status.into(),
            web_url: job.web_url,
            duration: job.duration,
        }
    }
}

pub(crate) const fn state_param(state: StateFilter) -> &'static str {
    match state {
        StateFilter::Opened => "opened",
        StateFilter::Merged => "merged",
        StateFilter::Closed => "closed",
        StateFilter::All => "all",
    }
}

pub(crate) const fn scope_param(scope: Scope) -> &'static str {
    match scope {
        Scope::All => "all",
        Scope::CreatedByMe => "created_by_me",
        Scope::AssignedToMe => "assigned_to_me",
    }
}

#[derive(Serialize)]
pub(crate) struct MergeRequestBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<&'a str>,
    squash: bool,
    should_remove_source_branch: bool,
}

impl<'a> From<&'a MergeOptions> for MergeRequestBody<'a> {
    fn from(options: &'a MergeOptions) -> Self {
        Self {
            sha: options.sha.as_ref().map(|sha| sha.0.as_str()),
            squash: options.squash,
            should_remove_source_branch: options.should_remove_source_branch,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct NoteRequest<'a> {
    pub(crate) body: &'a str,
}

#[derive(Serialize)]
pub(crate) struct ResolveRequest {
    pub(crate) resolved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MERGE_REQUEST: &str = include_str!("../fixtures/merge_request.json");
    const DISCUSSIONS: &str = include_str!("../fixtures/discussions.json");
    const PIPELINES: &str = include_str!("../fixtures/pipelines.json");

    fn parse<J: serde::de::DeserializeOwned, T: From<J>>(json: &str) -> serde_json::Result<T> {
        serde_json::from_str::<J>(json).map(T::from)
    }

    fn parse_list<J: serde::de::DeserializeOwned, T: From<J>>(
        json: &str,
    ) -> serde_json::Result<Vec<T>> {
        let items: Vec<J> = serde_json::from_str(json)?;
        Ok(items.into_iter().map(T::from).collect())
    }

    #[test]
    fn a_merge_request_parses_with_unknown_fields_and_states() -> serde_json::Result<()> {
        let request: MergeRequest = parse::<MergeRequestJson, _>(MERGE_REQUEST)?;
        assert_eq!(request.iid, MergeRequestIid(42));
        assert_eq!(request.state, MergeRequestState::Opened);
        assert_eq!(request.detailed_merge_status, Some(MergeStatus::Other));
        assert_eq!(request.reviewers[0].username, "reviewer");
        assert_eq!(request.sha, Some(CommitSha("abc123".to_owned())));
        let pipeline = request
            .head_pipeline
            .as_ref()
            .map(|p| (p.status, p.duration));
        assert_eq!(pipeline, Some((CiStatus::Success, Some(252))));
        assert_eq!(
            request.diff_refs.map(|refs| refs.base_sha),
            Some(CommitSha("base000".to_owned()))
        );
        Ok(())
    }

    #[test]
    fn discussions_keep_their_line_positions() -> serde_json::Result<()> {
        let discussions: Vec<Discussion> = parse_list::<DiscussionJson, _>(DISCUSSIONS)?;
        let note = &discussions[1].notes[0];
        assert!(note.resolvable);
        assert_eq!(note.position.as_ref().and_then(|at| at.new_line), Some(12));
        assert!(discussions[0].notes[0].position.is_none());
        assert!(!discussions[0].is_resolvable());
        assert!(discussions[1].is_resolvable());
        assert!(!discussions[1].is_resolved());
        Ok(())
    }

    #[test]
    fn pipelines_name_their_ref() -> serde_json::Result<()> {
        let pipelines: Vec<Pipeline> = parse_list::<PipelineJson, _>(PIPELINES)?;
        assert_eq!(pipelines[0].git_ref, "feature");
        assert_eq!(pipelines[0].status, CiStatus::Success);
        assert_eq!(pipelines[1].status, CiStatus::Other);
        Ok(())
    }

    #[test]
    fn merge_options_leave_out_an_unset_sha() -> serde_json::Result<()> {
        let options = MergeOptions::default();
        let body = serde_json::to_value(MergeRequestBody::from(&options))?;
        assert!(body.get("sha").is_none());
        Ok(())
    }
}
