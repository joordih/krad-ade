use krad_domain::{EnvironmentName, FilePatch, GitObjectId, SessionId};
use krad_host::HostLink;
use krad_protocol::RequestId;
use krad_repo::{repo_root, session_diff, snapshot_tree, RepoError};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct SessionRead {
    pub(crate) repo: PathBuf,
    pub(crate) baseline: GitObjectId,
    pub(crate) files: Vec<FilePatch>,
}

#[derive(Debug)]
pub(crate) enum ChangesDone {
    Captured {
        environment: EnvironmentName,
        session: SessionId,
        baseline: Result<Option<GitObjectId>, RepoError>,
    },
    Read {
        request: RequestId,
        environment: EnvironmentName,
        session: SessionId,
        read: Result<Option<SessionRead>, RepoError>,
    },
}

#[tracing::instrument(skip_all)]
pub(crate) fn capture_baseline(
    link: &HostLink,
    root: &Path,
) -> Result<Option<GitObjectId>, RepoError> {
    let Some(repo) = repo_root(link, root)? else {
        return Ok(None);
    };
    snapshot_tree(link, &repo).map(Some)
}

#[tracing::instrument(skip_all)]
pub(crate) fn read_session(
    link: &HostLink,
    root: &Path,
    baseline: Option<GitObjectId>,
) -> Result<Option<SessionRead>, RepoError> {
    let Some(repo) = repo_root(link, root)? else {
        return Ok(None);
    };
    let baseline = match baseline {
        Some(baseline) => baseline,
        None => snapshot_tree(link, &repo)?,
    };
    let diff = session_diff(link, &repo, &baseline)?;
    Ok(Some(SessionRead {
        repo,
        baseline,
        files: diff.files,
    }))
}
