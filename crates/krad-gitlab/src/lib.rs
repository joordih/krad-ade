mod client;
mod config;
mod encode;
mod model;
mod oauth;
mod pkce;

pub use client::{GitlabClient, GitlabError, TokenSaver};
pub use config::{ConfigError, GitlabConfig};
pub use krad_domain::review::{
    project_on_host, remote_project, Approval, Approvals, Branch, Change, CiStatus, Commit,
    CommitSha, DiffRefs, Discussion, DiscussionId, InvalidProjectPath, Job, JobId, MergeOptions,
    MergeRequest, MergeRequestIid, MergeRequestQuery, MergeRequestState, MergeStatus, Note, NoteId,
    NotePosition, Pipeline, PipelineId, ProjectPath, RemoteProject, Scope, StateFilter, TreeEntry,
    TreeEntryKind, User, UserId,
};
pub use krad_domain::GitlabTokens;
pub use oauth::{DeviceLogin, LoopbackLogin, OAuthError, REDIRECT_URI, SCOPE};
