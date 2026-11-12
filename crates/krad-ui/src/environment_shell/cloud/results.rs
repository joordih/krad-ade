mod request;

use gpui_kit::{AppContext as _, Context};
use krad_domain::review::MergeRequest;
use krad_domain::{
    AppEvent, CloudPlacement, CodeHost, EnvironmentName, GroupId, SessionId, ToastKind,
    WorktreeOrigin,
};
use krad_gitlab::GitlabConfig;
use krad_host::HostLink;
use krad_repo::{
    add_branch_worktree, fetch, push, resolved_remote_url, Askpass, Branch, BranchPlace,
    PushTarget, RepoError, Worktree, ORIGIN_REMOTE,
};
use std::fmt;
use std::path::{Path, PathBuf};

use super::super::agent_bus::AskpassPlan;
use super::super::review_view::{repo_of_remote, CodeHostError};
use super::super::EnvironmentShell;
use super::naming::{upstream_branch, upstream_remote};
use request::{commit_subjects, default_branch, new_request};

const NOT_CLOUD: &str = "This session does not run in the cloud";
const NOT_READY: &str = "The cloud runner is not connected yet; try again once it is ready";
const NO_REPO: &str = "The group's repository is not known yet";
const PUSHING: &str = "Pushing the session's branch from the runner…";
const PUSHED_PREFIX: &str = "Pushed";
const OPENING: &str = "Opening a pull request for the session…";
const OPENED_PREFIX: &str = "Opened";
const BRINGING: &str = "Bringing the session's branch to this computer…";
const BROUGHT_PREFIX: &str = "The branch is now a worktree group:";
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
enum ResultError {
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

struct CloudWork {
    env: EnvironmentName,
    group: Option<GroupId>,
    name: String,
    placement: CloudPlacement,
    link: HostLink,
    upstream: Option<String>,
}

fn push_branch(link: &HostLink, placement: &CloudPlacement) -> Result<(), ResultError> {
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
fn open_request(
    work: &CloudWork,
    gitlab: Option<GitlabConfig>,
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
        .client()
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

struct LocalRepo {
    link: HostLink,
    root: PathBuf,
    remote: String,
    askpass: AskpassPlan,
}

fn bring_home(work: &CloudWork, local: LocalRepo) -> Result<Worktree, ResultError> {
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

impl EnvironmentShell {
    fn cloud_work(
        &mut self,
        session: Option<SessionId>,
        cx: &mut Context<Self>,
    ) -> Option<CloudWork> {
        let env = self.active.clone();
        let live = self.envs.get(&env)?;
        let found = match session {
            Some(session) => live.data.session(session),
            None => live.data.current_session(),
        }?;
        let Some(placement) = found.cloud.clone() else {
            self.toast(ToastKind::Warning, NOT_CLOUD, cx);
            return None;
        };
        let group = found.group;
        let name = found.name.clone();
        let upstream = group
            .and_then(|group| live.data.group(group))
            .and_then(|group| self.git_at(&env, &group.root))
            .and_then(|git| git.status.upstream.clone());
        let Some(link) = self.runner_link(placement.runner) else {
            self.ensure_runner_ready(placement.runner, cx);
            self.toast(ToastKind::Warning, NOT_READY, cx);
            return None;
        };
        Some(CloudWork {
            env,
            group,
            name,
            placement,
            link,
            upstream,
        })
    }

    pub(in crate::environment_shell) fn push_cloud_branch(
        &mut self,
        session: Option<SessionId>,
        cx: &mut Context<Self>,
    ) {
        let Some(work) = self.cloud_work(session, cx) else {
            return;
        };
        self.toast(ToastKind::Info, PUSHING, cx);
        let branch = work.placement.branch.clone();
        let pushed = cx.background_spawn(async move { push_branch(&work.link, &work.placement) });
        cx.spawn(async move |this, cx| {
            let pushed = pushed.await;
            this.update(cx, |shell, cx| match pushed {
                Ok(()) => shell.toast(ToastKind::Success, &format!("{PUSHED_PREFIX} {branch}"), cx),
                Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn open_cloud_request(
        &mut self,
        session: Option<SessionId>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut work) = self.cloud_work(session, cx) else {
            return;
        };
        work.upstream = upstream_branch(work.upstream.as_deref()).map(str::to_owned);
        self.toast(ToastKind::Info, OPENING, cx);
        let gitlab = GitlabConfig::try_from(&self.settings.gitlab).ok();
        let (env, group) = (work.env.clone(), work.group);
        let opened = cx.background_spawn(async move { open_request(&work, gitlab) });
        cx.spawn(async move |this, cx| {
            let opened = opened.await;
            this.update(cx, |shell, cx| match opened {
                Ok(request) => {
                    cx.open_url(&request.web_url);
                    let text = format!("{OPENED_PREFIX} {}", request.web_url);
                    shell.toast(ToastKind::Success, &text, cx);
                    shell.publish_event(
                        &env,
                        group,
                        AppEvent::MergeRequestOpened {
                            merge_request: request.iid,
                        },
                    );
                }
                Err(error) => {
                    tracing::info!(%error, "no request was opened for the cloud session");
                    shell.toast(ToastKind::Error, &error.to_string(), cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn bring_cloud_home(
        &mut self,
        session: Option<SessionId>,
        cx: &mut Context<Self>,
    ) {
        let Some(work) = self.cloud_work(session, cx) else {
            return;
        };
        let root = work
            .group
            .and_then(|group| self.envs.get(&work.env)?.data.group(group))
            .and_then(|group| self.git_at(&work.env, &group.root))
            .map(|git| git.repo_root.clone());
        let Some(root) = root else {
            return self.toast(ToastKind::Warning, NO_REPO, cx);
        };
        let local = LocalRepo {
            link: self.env_link(&work.env),
            remote: upstream_remote(work.upstream.as_deref())
                .unwrap_or(ORIGIN_REMOTE)
                .to_owned(),
            askpass: self.git_askpass(&work.env, &root),
            root,
        };
        self.toast(ToastKind::Info, BRINGING, cx);
        let env = work.env.clone();
        let branch = work.placement.branch.clone();
        let repo_root = local.root.clone();
        let brought = cx.background_spawn(async move { bring_home(&work, local) });
        cx.spawn(async move |this, cx| {
            let brought = brought.await;
            this.update(cx, |shell, cx| match brought {
                Ok(worktree) => {
                    let text = format!("{BROUGHT_PREFIX} {branch}");
                    let origin = WorktreeOrigin { repo_root, branch };
                    shell.add_group_to(&env, worktree.path, Some(origin), cx);
                    shell.toast(ToastKind::Success, &text, cx);
                }
                Err(error) => shell.toast(ToastKind::Error, &error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }
}
