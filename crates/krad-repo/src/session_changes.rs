use krad_domain::GitObjectId;
use krad_host::HostLink;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::file_ops::delete_entry;
use crate::files::read_text;
use crate::git::{required_repo_root, run_git, run_git_within};
use crate::patch::{parse_patch, revert_hunk, FilePatch, PatchBody, PatchStatus};
use crate::process::host_path;
use crate::RepoError;

const INDEX_FILE_VAR: &str = "GIT_INDEX_FILE";
const GIT_PATH_ARGS: [&str; 3] = ["rev-parse", "--git-path", "index"];
const SCRATCH_SUFFIX: &str = "krad-session";
const ADD_ALL_ARGS: [&str; 2] = ["add", "--all"];
const WRITE_TREE_ARGS: [&str; 1] = ["write-tree"];
const SNAPSHOT_WAIT: Duration = Duration::from_mins(2);
const SESSION_DIFF_ARGS: [&str; 11] = [
    "diff",
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--no-renames",
    "--no-relative",
    "--full-index",
    "--binary",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--ignore-submodules=none",
];
const HASH_OBJECT_ARGS: [&str; 2] = ["hash-object", "--"];
const CAT_FILE_ARGS: [&str; 2] = ["cat-file", "--filters"];
const CAT_FILE_PATH: &str = "--path=";
const MAX_HUNK_FILE_BYTES: u64 = 8 * 1024 * 1024;

static SCRATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionDiff {
    pub current: GitObjectId,
    pub files: Vec<FilePatch>,
}

fn object_id(stdout: &[u8], command: &str) -> Result<GitObjectId, RepoError> {
    String::from_utf8_lossy(stdout)
        .parse()
        .map_err(|_| RepoError::Git {
            command: command.to_owned(),
            message: String::from_utf8_lossy(stdout).trim().to_owned(),
        })
}

fn on_host(host: &HostLink, repo: &Path, printed: &str) -> PathBuf {
    let path = Path::new(printed);
    if path.is_relative() {
        repo.join(path)
    } else {
        host_path(host, repo, printed)
    }
}

/// # Errors
///
/// Returns [`RepoError::Git`] when git cannot record the working tree, as
/// outside a repository, and [`RepoError::Io`] when the scratch index cannot
/// be written or removed.
#[must_use = "the tree is the only handle on the snapshot"]
pub fn snapshot_tree(host: &HostLink, repo: &Path) -> Result<GitObjectId, RepoError> {
    let printed = String::from_utf8_lossy(&run_git(host, repo, &GIT_PATH_ARGS)?)
        .trim_end()
        .to_owned();
    let index = on_host(host, repo, &printed);
    let serial = SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    let scratch_printed = format!("{printed}.{SCRATCH_SUFFIX}-{}-{serial}", std::process::id());
    let scratch = on_host(host, repo, &scratch_printed);
    if host.exists(&index) {
        host.write(&scratch, &host.read_all(&index)?)?;
    }
    let env = [(INDEX_FILE_VAR.to_owned(), OsString::from(&scratch_printed))];
    let tree = run_git_within(host, repo, &ADD_ALL_ARGS, &env, SNAPSHOT_WAIT)
        .and_then(|_| run_git_within(host, repo, &WRITE_TREE_ARGS, &env, SNAPSHOT_WAIT))
        .and_then(|stdout| object_id(&stdout, WRITE_TREE_ARGS[0]));
    let removed = if host.exists(&scratch) {
        host.remove_file(&scratch).map_err(RepoError::from)
    } else {
        Ok(())
    };
    let tree = tree?;
    removed?;
    Ok(tree)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when the working tree cannot be recorded or the
/// baseline is no longer in the repository.
pub fn session_diff(
    host: &HostLink,
    repo: &Path,
    baseline: &GitObjectId,
) -> Result<SessionDiff, RepoError> {
    let current = snapshot_tree(host, repo)?;
    let mut args: Vec<String> = SESSION_DIFF_ARGS
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
    args.extend([baseline.to_string(), current.to_string()]);
    let stdout = run_git(host, repo, &args)?;
    Ok(SessionDiff {
        current,
        files: parse_patch(&String::from_utf8_lossy(&stdout)),
    })
}

fn current_object(
    host: &HostLink,
    repo: &Path,
    path: &Path,
) -> Result<Option<GitObjectId>, RepoError> {
    if !host.exists(&repo.join(path)) {
        return Ok(None);
    }
    let mut args: Vec<OsString> = HASH_OBJECT_ARGS.iter().map(OsString::from).collect();
    args.push(path.as_os_str().to_owned());
    let stdout = run_git(host, repo, &args)?;
    object_id(&stdout, HASH_OBJECT_ARGS[0]).map(Some)
}

fn stale(patch: &FilePatch) -> RepoError {
    RepoError::Stale(patch.path.display().to_string())
}

/// # Errors
///
/// Returns [`RepoError::Stale`] when the file no longer matches the diff the
/// user saw, [`RepoError::Unsupported`] for a submodule or a rename,
/// [`RepoError::OutsideRepository`] when `repo` is not inside a repository, and
/// [`RepoError::Git`] or [`RepoError::Io`] when it cannot be restored.
pub fn revert_session_file(
    host: &HostLink,
    repo: &Path,
    patch: &FilePatch,
) -> Result<(), RepoError> {
    if matches!(patch.body, PatchBody::Submodule | PatchBody::Renamed { .. }) {
        return Err(RepoError::Unsupported(patch.path.display().to_string()));
    }
    let root = required_repo_root(host, repo)?;
    let repo = root.as_path();
    if current_object(host, repo, &patch.path)? != patch.new_id {
        return Err(stale(patch));
    }
    let target = repo.join(&patch.path);
    let Some(old) = patch
        .old_id
        .as_ref()
        .filter(|_| patch.status != PatchStatus::Added)
    else {
        return delete_entry(host, &target);
    };
    let mut path_arg = OsString::from(CAT_FILE_PATH);
    path_arg.push(patch.path.as_os_str());
    let mut args: Vec<OsString> = CAT_FILE_ARGS.iter().map(OsString::from).collect();
    args.extend([path_arg, OsString::from(old.to_string())]);
    let bytes = run_git(host, repo, &args)?;
    if let Some(parent) = target.parent() {
        host.create_dir_all(parent)?;
    }
    host.write(&target, &bytes)?;
    Ok(())
}

/// # Errors
///
/// Returns [`RepoError::Stale`] when the hunk's lines changed since its diff
/// was read, [`RepoError::Unsupported`] for a file that is not text, and
/// [`RepoError::Io`] when the file cannot be read or written.
pub fn revert_session_hunk(
    host: &HostLink,
    repo: &Path,
    patch: &FilePatch,
    hunk: usize,
) -> Result<(), RepoError> {
    let PatchBody::Text(hunks) = &patch.body else {
        return Err(RepoError::Unsupported(patch.path.display().to_string()));
    };
    let chosen = hunks.get(hunk).ok_or_else(|| stale(patch))?;
    if patch.status != PatchStatus::Modified {
        return revert_session_file(host, repo, patch);
    }
    let target = required_repo_root(host, repo)?.join(&patch.path);
    let file = read_text(host, &target, MAX_HUNK_FILE_BYTES)?;
    if file.truncated {
        return Err(RepoError::Unsupported(patch.path.display().to_string()));
    }
    let text = revert_hunk(&file.text, chosen).map_err(|_| stale(patch))?;
    host.write(&target, text.as_bytes())?;
    Ok(())
}
