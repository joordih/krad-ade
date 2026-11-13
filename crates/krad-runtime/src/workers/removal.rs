use futures::channel::oneshot;
use krad_agents::{ApiError, Closed, ErrorCode, Response, WorktreeOutcome};
use krad_domain::{EnvironmentName, GroupId};
use krad_host::HostLink;
use krad_repo::{remove_settled_worktree, RepoError};
use std::path::PathBuf;

pub(crate) const WORKTREE_IN_USE: &str = "Other sessions still use that worktree, so it was kept";

#[derive(Debug)]
pub(crate) struct Removal {
    pub(crate) group: GroupId,
    pub(crate) repo_root: PathBuf,
    pub(crate) path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct LeadRemovals {
    pub(crate) environment: EnvironmentName,
    pub(crate) closed: Vec<Closed>,
    pub(crate) removed: Vec<(Removal, Result<(), RepoError>)>,
    pub(crate) reply: oneshot::Sender<Response>,
}

#[must_use]
pub fn removal_outcome(removed: Result<(), RepoError>) -> WorktreeOutcome {
    match removed {
        Ok(()) => WorktreeOutcome::Removed,
        Err(error @ (RepoError::UncommittedChanges | RepoError::UnmergedCommits(_))) => {
            WorktreeOutcome::Refused(ApiError::new(
                ErrorCode::WorktreeNotClean,
                error.to_string(),
            ))
        }
        Err(error) => WorktreeOutcome::Refused(ApiError::new(ErrorCode::Failed, error.to_string())),
    }
}

#[tracing::instrument(skip_all, fields(removals = removals.len()))]
pub(crate) fn remove_settled(
    link: &HostLink,
    removals: Vec<Removal>,
) -> Vec<(Removal, Result<(), RepoError>)> {
    removals
        .into_iter()
        .map(|removal| {
            let removed = remove_settled_worktree(link, &removal.repo_root, &removal.path);
            (removal, removed)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worktree_with_work_left_is_refused_and_other_failures_say_so() {
        assert_eq!(removal_outcome(Ok(())), WorktreeOutcome::Removed);
        for unsettled in [RepoError::UncommittedChanges, RepoError::UnmergedCommits(2)] {
            assert!(matches!(
                removal_outcome(Err(unsettled)),
                WorktreeOutcome::Refused(ApiError {
                    code: ErrorCode::WorktreeNotClean,
                    ..
                })
            ));
        }
        let failed = removal_outcome(Err(RepoError::Unsupported("x".to_owned())));
        assert!(matches!(
            failed,
            WorktreeOutcome::Refused(ApiError {
                code: ErrorCode::Failed,
                ..
            })
        ));
    }
}
