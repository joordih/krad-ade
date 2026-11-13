mod request;

use krad_domain::review::MergeRequest;
use krad_domain::{CloudPlacement, CodeHost, EnvironmentName, GroupId};
use krad_gitlab::GitlabConfig;
use krad_host::HostLink;
use krad_repo::{
    add_branch_worktree, fetch, push, resolved_remote_url, Askpass, Branch, BranchPlace,
    PushTarget, RepoError, Worktree, ORIGIN_REMOTE,
};
use krad_store::Secrets;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::code_hosts::{repo_of_remote, HostError as CodeHostError};
use crate::git::AskpassPlan;
use request::{commit_subjects, default_branch, new_request};

const PUSH_FAILED: &str = "The runner could not push the branch";
const NO_REMOTE: &str = "The runner's clone has no remote";
const NOT_ON_HOST: &str = "The remote is neither on GitHub nor on the GitLab in Settings";
const SIGNED_OUT_GITHUB: &str = "Sign in to GitHub in Settings to open pull requests";
const SIGNED_OUT_GITLAB: &str = "Sign in to GitLab in Settings to open merge requests";
const NO_TARGET: &str = "Could not tell which branch the request merges into";
const VAULT_FAILED: &str = "Could not read the code host login";
const FETCH_FAILED: &str = "Could not fetch the branch here";
const WORKTREE_FAILED: &str = "Could not open the branch as a worktree";

#[derive(Debug)]
pub(crate) enum ResultError {
    Push(RepoError),
    NoRemote,
    NotOnHost,
    SignedOut(CodeHost),
    NoTarget,
    Vault(String),
    CodeHost(CodeHostError),
    Fetch(RepoError),
    Worktree(RepoError),
}

impl fmt::Display for ResultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Push(error) => write!(f, "{PUSH_FAILED}: {error}"),
            Self::NoRemote => f.write_str(NO_REMOTE),
            Self::NotOnHost => f.write_str(NOT_ON_HOST),
            Self::SignedOut(CodeHost::Github) => f.write_str(SIGNED_OUT_GITHUB),
            Self::SignedOut(CodeHost::Gitlab) => f.write_str(SIGNED_OUT_GITLAB),
            Self::NoTarget => f.write_str(NO_TARGET),
            Self::Vault(error) => write!(f, "{VAULT_FAILED}: {error}"),
            Self::CodeHost(error) => error.fmt(f),
            Self::Fetch(error) => write!(f, "{FETCH_FAILED}: {error}"),
            Self::Worktree(error) => write!(f, "{WORKTREE_FAILED}: {error}"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct CloudWork {
    pub(crate) env: EnvironmentName,
    pub(crate) group: Option<GroupId>,
    pub(crate) name: String,
    pub(crate) placement: CloudPlacement,
    pub(crate) link: HostLink,
    pub(crate) upstream: Option<String>,
}

pub(crate) fn push_branch(link: &HostLink, placement: &CloudPlacement) -> Result<(), ResultError> {
    let target = PushTarget::Publish {
        remote: ORIGIN_REMOTE.to_owned(),
        branch: placement.branch.clone(),
    };
    push(link, &placement.worktree, &target, &Askpass::default()).map_err(ResultError::Push)
}

fn runner_home(link: &HostLink) -> Option<PathBuf> {
    match link {
        HostLink::Remote(ssh) => ssh.home().map(Path::to_path_buf),
        HostLink::Local => None,
    }
}

#[tracing::instrument(skip_all, fields(runner = %work.placement.runner))]
pub(crate) fn open_request(
    work: &CloudWork,
    gitlab: Option<GitlabConfig>,
    secrets: &Secrets,
) -> Result<MergeRequest, ResultError> {
    let CloudWork {
        link, placement, ..
    } = work;
    push_branch(link, placement)?;
    let home = runner_home(link);
    let url = resolved_remote_url(link, &placement.repo_dir, ORIGIN_REMOTE, home.as_deref())
        .map_err(|_| ResultError::NoRemote)?
        .ok_or(ResultError::NoRemote)?;
    let repo = repo_of_remote(&url, gitlab).ok_or(ResultError::NotOnHost)?;
    let client = repo
        .login
        .client(secrets)
        .map_err(|error| ResultError::Vault(error.to_string()))?
        .ok_or_else(|| ResultError::SignedOut(repo.login.code_host()))?;
    let target = work
        .upstream
        .clone()
        .or_else(|| default_branch(link, &placement.repo_dir))
        .ok_or(ResultError::NoTarget)?;
    let subjects = commit_subjects(link, &placement.worktree, &target);
    let request = new_request(&work.name, &placement.branch, &target, &subjects);
    client
        .create_merge_request(&repo.project, &request)
        .map_err(ResultError::CodeHost)
}

#[derive(Debug)]
pub(crate) struct LocalRepo {
    pub(crate) link: HostLink,
    pub(crate) root: PathBuf,
    pub(crate) remote: String,
    pub(crate) askpass: AskpassPlan,
}

pub(crate) fn bring_home(work: &CloudWork, local: LocalRepo) -> Result<Worktree, ResultError> {
    push_branch(&work.link, &work.placement)?;
    let askpass = local.askpass.resolve();
    fetch(&local.link, &local.root, &askpass).map_err(ResultError::Fetch)?;
    let branch = Branch {
        name: work.placement.branch.clone(),
        place: BranchPlace::Remote(local.remote.clone()),
        upstream: None,
        is_head: false,
    };
    add_branch_worktree(&local.link, &local.root, &branch).map_err(ResultError::Worktree)
}
