use krad_domain::{CloudPlacement, CloudRunnerId};
use krad_host::{remote_path, HostCommand, HostError, HostLink};
use std::fmt;
use std::path::Path;
use std::process::Output;
use std::time::Duration;

use super::naming::{placement_for, repo_slug};

const GIT_PROGRAM: &str = "git";
const DIR_FLAG: &str = "-C";
const PROMPT_VAR: &str = "GIT_TERMINAL_PROMPT";
const PROMPT_DISABLED: &str = "0";
const CLONE_WAIT: Duration = Duration::from_secs(600);
const GIT_WAIT: Duration = Duration::from_secs(120);
const GIT_MARKER: &str = ".git";
const ORIGIN: &str = "origin";
const DEFAULT_BASE: &str = "origin/HEAD";
const REMOTE_REFS: &str = "refs/remotes/origin/";
const PATCH_SUFFIX: &str = ".patch";
const NO_LAYOUT: &str = "the runner's home folder is not known yet";
const CLONE_FAILED: &str = "The runner could not clone";
const FETCH_FAILED: &str = "The runner could not fetch";
const WORKTREE_FAILED: &str = "The runner could not create the session's worktree";
const APPLY_FAILED: &str = "Your uncommitted changes could not be applied on the runner";
const HOST_FAILED: &str = "The runner could not be reached";

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct WorkspaceRequest {
    pub(in crate::environment_shell) runner: CloudRunnerId,
    pub(in crate::environment_shell) remote_url: String,
    pub(in crate::environment_shell) upstream_branch: Option<String>,
    pub(in crate::environment_shell) session_slug: String,
    pub(in crate::environment_shell) patch: Option<String>,
}

#[derive(Debug)]
pub(in crate::environment_shell) struct PreparedWorkspace {
    pub(in crate::environment_shell) placement: CloudPlacement,
    pub(in crate::environment_shell) patch_problem: Option<String>,
}

#[derive(Debug)]
pub(in crate::environment_shell) enum WorkspaceError {
    NoLayout,
    Clone(String),
    Fetch(String),
    Worktree(String),
    Host(HostError),
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLayout => formatter.write_str(NO_LAYOUT),
            Self::Clone(stderr) => write!(formatter, "{CLONE_FAILED}: {stderr}"),
            Self::Fetch(stderr) => write!(formatter, "{FETCH_FAILED}: {stderr}"),
            Self::Worktree(stderr) => write!(formatter, "{WORKTREE_FAILED}: {stderr}"),
            Self::Host(error) => write!(formatter, "{HOST_FAILED}: {error}"),
        }
    }
}

impl From<HostError> for WorkspaceError {
    fn from(error: HostError) -> Self {
        Self::Host(error)
    }
}

pub(super) fn git(dir: Option<&Path>) -> HostCommand {
    let mut command = HostCommand::new(GIT_PROGRAM);
    if let Some(dir) = dir {
        command.args([DIR_FLAG, &remote_path(dir)]);
    }
    command.env(PROMPT_VAR, PROMPT_DISABLED);
    command
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

fn run(link: &HostLink, command: &HostCommand, wait: Duration) -> Result<Output, HostError> {
    link.output_within(command, wait)
}

fn ensure_clone(
    link: &HostLink,
    request: &WorkspaceRequest,
    repo: &Path,
) -> Result<(), WorkspaceError> {
    if link.exists(&repo.join(GIT_MARKER)) {
        let mut fetch = git(Some(repo));
        fetch.args(["fetch", "--prune", ORIGIN]);
        let output = run(link, &fetch, CLONE_WAIT)?;
        return if output.status.success() {
            Ok(())
        } else {
            Err(WorkspaceError::Fetch(stderr_of(&output)))
        };
    }
    if let Some(parent) = repo.parent() {
        link.create_dir_all(parent)?;
    }
    let mut clone = git(None);
    clone.args(["clone", "--quiet", &request.remote_url, &remote_path(repo)]);
    let output = run(link, &clone, CLONE_WAIT)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(WorkspaceError::Clone(stderr_of(&output)))
    }
}

fn base_of(link: &HostLink, repo: &Path, upstream: Option<&str>) -> Result<String, HostError> {
    let Some(branch) = upstream else {
        return Ok(DEFAULT_BASE.to_owned());
    };
    let mut verify = git(Some(repo));
    verify.args([
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{REMOTE_REFS}{branch}"),
    ]);
    let found = run(link, &verify, GIT_WAIT)?.status.success();
    Ok(if found {
        format!("{ORIGIN}/{branch}")
    } else {
        DEFAULT_BASE.to_owned()
    })
}

fn add_worktree(
    link: &HostLink,
    placement: &CloudPlacement,
    base: &str,
) -> Result<(), WorkspaceError> {
    if let Some(parent) = placement.worktree.parent() {
        link.create_dir_all(parent)?;
    }
    let mut add = git(Some(&placement.repo_dir));
    add.args([
        "worktree",
        "add",
        "--no-track",
        "-b",
        &placement.branch,
        &remote_path(&placement.worktree),
        base,
    ]);
    let output = run(link, &add, GIT_WAIT)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(WorkspaceError::Worktree(stderr_of(&output)))
    }
}

fn apply_patch(link: &HostLink, worktree: &Path, patch: &str) -> Option<String> {
    let mut file = worktree.as_os_str().to_owned();
    file.push(PATCH_SUFFIX);
    let file = std::path::PathBuf::from(file);
    if let Err(error) = link.write(&file, patch.as_bytes()) {
        return Some(format!("{APPLY_FAILED}: {error}"));
    }
    let mut apply = git(Some(worktree));
    apply.args(["apply", "--whitespace=nowarn", &remote_path(&file)]);
    let problem = match run(link, &apply, GIT_WAIT) {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(format!("{APPLY_FAILED}: {}", stderr_of(&output))),
        Err(error) => Some(format!("{APPLY_FAILED}: {error}")),
    };
    if let Err(error) = link.remove_file(&file) {
        tracing::debug!(%error, "the uploaded patch stayed on the runner");
    }
    problem
}

#[tracing::instrument(skip_all, fields(runner = %request.runner))]
pub(in crate::environment_shell) fn prepare_workspace(
    link: &HostLink,
    request: &WorkspaceRequest,
) -> Result<PreparedWorkspace, WorkspaceError> {
    let HostLink::Remote(ssh) = link else {
        return Err(WorkspaceError::NoLayout);
    };
    let layout = ssh.layout().ok_or(WorkspaceError::NoLayout)?;
    let placement = placement_for(
        request.runner,
        &layout.runner_dir(),
        &repo_slug(&request.remote_url),
        &request.session_slug,
    );
    ensure_clone(link, request, &placement.repo_dir)?;
    let base = base_of(
        link,
        &placement.repo_dir,
        request.upstream_branch.as_deref(),
    )?;
    add_worktree(link, &placement, &base)?;
    let patch_problem = request
        .patch
        .as_deref()
        .and_then(|patch| apply_patch(link, &placement.worktree, patch));
    tracing::info!("cloud session workspace ready");
    Ok(PreparedWorkspace {
        placement,
        patch_problem,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_on_the_runner_never_prompts_and_works_in_its_folder() {
        let command = git(Some(Path::new("/srv/repo")));
        let args: Vec<String> = command
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["-C", "/srv/repo"]);
        assert!(command
            .env
            .iter()
            .any(|(name, value)| name == PROMPT_VAR && value == PROMPT_DISABLED));
    }

    #[test]
    fn failures_say_what_the_runner_could_not_do() {
        let text = WorkspaceError::Clone("Permission denied (publickey)".to_owned()).to_string();
        assert_eq!(
            text,
            "The runner could not clone: Permission denied (publickey)"
        );
        assert!(WorkspaceError::Worktree("exists".to_owned())
            .to_string()
            .starts_with(WORKTREE_FAILED));
    }
}
