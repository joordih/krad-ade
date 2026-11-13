use futures::channel::oneshot;
use krad_agents::Response;
use krad_domain::{
    Branch, CommitMode, CommitScope, EnvironmentName, FileChange, FilePatch, GroupId, PushTarget,
    Worktree, WorktreeSettings,
};
use krad_host::HostLink;
use krad_protocol::RequestId;
use krad_repo::{
    add_branch_worktree, add_planned_worktree, add_worktree, check_out, commit, create_branch,
    create_tag, delete_tag, discard_change, discard_folder, fetch, pop_stash, pull, push,
    push_tags, remove_worktree, revert_session_file, revert_session_hunk, stage, stage_all,
    stash_changes, sync, unstage, unstage_all, RepoError,
};
use std::path::PathBuf;

use super::AskpassPlan;
use crate::runtime::RunPlan;
use crate::workers::PendingWorker;

#[derive(Debug)]
pub(crate) enum Act {
    Fetch,
    Pull,
    Push(PushTarget),
    Sync(PushTarget),
    PushTags,
    Stash,
    PopStash,
    Stage(Vec<PathBuf>),
    Unstage(Vec<PathBuf>),
    StageAll,
    UnstageAll,
    Commit {
        message: String,
        mode: CommitMode,
        scope: CommitScope,
        push: Option<PushTarget>,
    },
    CreateBranch(String),
    CreateTag(String),
    DeleteTag(String),
    CheckOut(Branch),
    Discard(FileChange),
    DiscardFolder(PathBuf),
    CreateWorktree {
        name: String,
        settings: WorktreeSettings,
    },
    OpenBranchWorktree(Branch),
    AddWorktree(String),
    RemoveWorktree {
        path: PathBuf,
    },
    RevertSession {
        patch: Box<FilePatch>,
        hunk: Option<usize>,
    },
}

#[derive(Debug)]
pub(crate) enum Asker {
    Client(RequestId),
    Worker(Box<PendingWorker>),
    Kraddie(Box<RunPlan>),
    Bus(oneshot::Sender<Response>),
}

#[derive(Debug)]
pub(crate) struct ActOrder {
    pub(crate) asker: Asker,
    pub(crate) environment: EnvironmentName,
    pub(crate) group: Option<GroupId>,
    pub(crate) link: HostLink,
    pub(crate) repo: PathBuf,
    pub(crate) askpass: AskpassPlan,
    pub(crate) act: Act,
}

#[derive(Debug)]
pub(crate) enum Acted {
    Done,
    Stale,
    Created(Worktree),
    Opened(Worktree),
    Removed,
}

#[derive(Debug)]
pub(crate) struct ActDone {
    pub(crate) asker: Asker,
    pub(crate) environment: EnvironmentName,
    pub(crate) group: Option<GroupId>,
    pub(crate) repo: PathBuf,
    pub(crate) acted: Result<Acted, RepoError>,
}

#[tracing::instrument(skip_all)]
pub(crate) fn run_act(order: ActOrder) -> ActDone {
    let ActOrder {
        asker,
        environment,
        group,
        link,
        repo,
        askpass,
        act,
    } = order;
    let acted = acting(&link, &repo, askpass, act);
    if let Err(error) = &acted {
        tracing::debug!(%error, "git act failed");
    }
    ActDone {
        asker,
        environment,
        group,
        repo,
        acted,
    }
}

fn acting(
    link: &HostLink,
    repo: &std::path::Path,
    askpass: AskpassPlan,
    act: Act,
) -> Result<Acted, RepoError> {
    let done = |result: Result<(), RepoError>| result.map(|()| Acted::Done);
    match act {
        Act::Fetch => done(fetch(link, repo, &askpass.resolve())),
        Act::Pull => done(pull(link, repo, &askpass.resolve())),
        Act::Push(target) => done(push(link, repo, &target, &askpass.resolve())),
        Act::Sync(target) => done(sync(link, repo, &target, &askpass.resolve())),
        Act::PushTags => done(push_tags(link, repo, &askpass.resolve())),
        Act::Stash => done(stash_changes(link, repo)),
        Act::PopStash => done(pop_stash(link, repo)),
        Act::Stage(paths) => done(stage(link, repo, &paths)),
        Act::Unstage(paths) => done(unstage(link, repo, &paths)),
        Act::StageAll => done(stage_all(link, repo)),
        Act::UnstageAll => done(unstage_all(link, repo)),
        Act::Commit {
            message,
            mode,
            scope,
            push: pushed,
        } => {
            commit(link, repo, &message, mode, scope)?;
            done(pushed.map_or(Ok(()), |target| {
                push(link, repo, &target, &askpass.resolve())
            }))
        }
        Act::CreateBranch(name) => done(create_branch(link, repo, &name)),
        Act::CreateTag(name) => done(create_tag(link, repo, &name)),
        Act::DeleteTag(name) => done(delete_tag(link, repo, &name)),
        Act::CheckOut(branch) => done(check_out(link, repo, &branch)),
        Act::Discard(change) => done(discard_change(link, repo, &change)),
        Act::DiscardFolder(folder) => discard_folder(link, repo, &folder).map(|_| Acted::Done),
        Act::CreateWorktree { name, settings } => {
            let planned = add_planned_worktree(link, repo, &name, &settings, &askpass.resolve())?;
            for setback in &planned.setbacks {
                tracing::warn!(%setback, "the new worktree skipped a step");
            }
            Ok(Acted::Created(planned.worktree))
        }
        Act::AddWorktree(branch) => add_worktree(link, repo, &branch).map(Acted::Created),
        Act::OpenBranchWorktree(branch) => {
            add_branch_worktree(link, repo, &branch).map(Acted::Opened)
        }
        Act::RemoveWorktree { path } => remove_worktree(link, repo, &path).map(|()| Acted::Removed),
        Act::RevertSession { patch, hunk } => {
            let reverted = match hunk {
                Some(hunk) => revert_session_hunk(link, repo, &patch, hunk),
                None => revert_session_file(link, repo, &patch),
            };
            match reverted {
                Ok(()) => Ok(Acted::Done),
                Err(RepoError::Stale(_)) => Ok(Acted::Stale),
                Err(error) => Err(error),
            }
        }
    }
}
