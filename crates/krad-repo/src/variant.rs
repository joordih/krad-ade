use krad_domain::{ChangedLines, DiffSummary, GitObjectId};
use krad_host::HostLink;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::git::run_git;
use crate::process::git_path;
use crate::session_changes::snapshot_tree;
use crate::status::status;
use crate::worktree::ensure_settled;
use crate::RepoError;

const HEAD_ARGS: [&str; 2] = ["rev-parse", "HEAD"];
const MERGE_BASE: &str = "merge-base";
const HEAD: &str = "HEAD";
const NUMSTAT_ARGS: [&str; 4] = ["diff", "--numstat", "-z", "--no-renames"];
const PATCH_ARGS: [&str; 7] = [
    "diff",
    "--binary",
    "--full-index",
    "--no-color",
    "--no-ext-diff",
    "--no-renames",
    "--no-textconv",
];
const GIT_PATH_ARGS: [&str; 2] = ["rev-parse", "--git-path"];
const PATCH_FILE: &str = "krad-variant.patch";
const APPLY: &str = "apply";
const APPLY_CHECK: &str = "--check";
const REMOVE_WORKTREE_ARGS: [&str; 2] = ["worktree", "remove"];
const FORCE_FLAG: &str = "--force";
const COUNT_BRANCH_UNMERGED_ARGS: [&str; 2] = ["rev-list", "--count"];
const NOT_FLAG: &str = "--not";
const ARG_SEPARATOR: &str = " ";
const PRUNE_ARGS: [&str; 2] = ["worktree", "prune"];
const BRANCH_EXISTS_ARGS: [&str; 3] = ["rev-parse", "--verify", "--quiet"];
const DELETE_BRANCH_ARGS: [&str; 2] = ["branch", "-D"];
const BRANCH_REF_PREFIX: &str = "refs/heads/";
const FIELD_SEPARATOR: char = '\t';
const RECORD_SEPARATOR: char = '\0';
const BINARY_COUNT: &str = "-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantRemoval {
    OnlyIfSettled,
    Forced,
}

static PATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantChanges {
    pub fork: GitObjectId,
    pub tree: GitObjectId,
}

fn object_id(stdout: &[u8], command: &str) -> Result<GitObjectId, RepoError> {
    let printed = String::from_utf8_lossy(stdout);
    printed.trim().parse().map_err(|_| RepoError::Git {
        command: command.to_owned(),
        message: printed.trim().to_owned(),
    })
}

/// # Errors
///
/// Returns [`RepoError::Git`] when either folder is not a checkout of the
/// same repository or the variant's working tree cannot be recorded.
pub fn variant_changes(
    host: &HostLink,
    base_root: &Path,
    variant_root: &Path,
) -> Result<VariantChanges, RepoError> {
    let base_head = object_id(&run_git(host, base_root, &HEAD_ARGS)?, HEAD_ARGS[0])?;
    let base = base_head.to_string();
    let fork = object_id(
        &run_git(host, variant_root, &[MERGE_BASE, HEAD, base.as_str()])?,
        MERGE_BASE,
    )?;
    let tree = snapshot_tree(host, variant_root)?;
    Ok(VariantChanges { fork, tree })
}

/// # Errors
///
/// As [`variant_changes`], and [`RepoError::Git`] when the diff cannot be
/// read.
pub fn variant_diff_summary(
    host: &HostLink,
    base_root: &Path,
    variant_root: &Path,
) -> Result<DiffSummary, RepoError> {
    let changes = variant_changes(host, base_root, variant_root)?;
    summary_of(host, variant_root, &changes)
}

fn summary_of(
    host: &HostLink,
    variant_root: &Path,
    changes: &VariantChanges,
) -> Result<DiffSummary, RepoError> {
    let (fork, tree) = (changes.fork.to_string(), changes.tree.to_string());
    let mut args: Vec<&str> = NUMSTAT_ARGS.to_vec();
    args.extend([fork.as_str(), tree.as_str()]);
    let stdout = run_git(host, variant_root, &args)?;
    Ok(DiffSummary::new(parse_numstat(&String::from_utf8_lossy(
        &stdout,
    ))))
}

fn parse_numstat(output: &str) -> Vec<ChangedLines> {
    output
        .split(RECORD_SEPARATOR)
        .filter_map(|record| {
            let mut fields = record.splitn(3, FIELD_SEPARATOR);
            let added = fields.next()?;
            let removed = fields.next()?;
            let path = fields.next().filter(|path| !path.is_empty())?;
            Some(ChangedLines {
                path: PathBuf::from(path),
                added: count(added),
                removed: count(removed),
            })
        })
        .collect()
}

fn count(field: &str) -> u32 {
    if field == BINARY_COUNT {
        return 0;
    }
    field.trim().parse().unwrap_or_default()
}

/// # Errors
///
/// Returns [`RepoError::Uncommitted`] when the base worktree has changes of
/// its own, and [`RepoError::Git`] when the variant's changes do not apply
/// cleanly onto it; in both cases the base is left untouched.
pub fn apply_variant(
    host: &HostLink,
    base_root: &Path,
    variant_root: &Path,
) -> Result<DiffSummary, RepoError> {
    if !status(host, base_root)?.changes.is_empty() {
        return Err(RepoError::Uncommitted(base_root.display().to_string()));
    }
    let changes = variant_changes(host, base_root, variant_root)?;
    let summary = summary_of(host, variant_root, &changes)?;
    let (fork, tree) = (changes.fork.to_string(), changes.tree.to_string());
    let mut args: Vec<&str> = PATCH_ARGS.to_vec();
    args.extend([fork.as_str(), tree.as_str()]);
    let patch = run_git(host, variant_root, &args)?;
    if patch.is_empty() {
        return Ok(summary);
    }
    let serial = PATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!("{PATCH_FILE}.{}-{serial}", std::process::id());
    let printed = String::from_utf8_lossy(&run_git(
        host,
        base_root,
        &[GIT_PATH_ARGS[0], GIT_PATH_ARGS[1], name.as_str()],
    )?)
    .trim_end()
    .to_owned();
    let file = if Path::new(&printed).is_relative() {
        base_root.join(&printed)
    } else {
        PathBuf::from(&printed)
    };
    host.write(&file, &patch)?;
    let target = git_path(host, base_root, &file);
    let applied = run_git(
        host,
        base_root,
        &[
            OsStr::new(APPLY),
            OsStr::new(APPLY_CHECK),
            target.as_os_str(),
        ],
    )
    .and_then(|_| run_git(host, base_root, &[OsStr::new(APPLY), target.as_os_str()]));
    let removed = host.remove_file(&file).map_err(RepoError::from);
    applied?;
    removed?;
    Ok(summary)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when git cannot remove the worktree or delete
/// its branch. A worktree or branch that is already gone is not an error.
/// With [`VariantRemoval::OnlyIfSettled`] it returns
/// [`RepoError::UncommittedChanges`] or [`RepoError::UnmergedCommits`] and
/// removes nothing when the worktree or its branch holds work.
pub fn discard_variant(
    host: &HostLink,
    repo_root: &Path,
    worktree: &Path,
    branch: &str,
    removal: VariantRemoval,
) -> Result<(), RepoError> {
    let reference = format!("{BRANCH_REF_PREFIX}{branch}");
    let mut exists: Vec<&str> = BRANCH_EXISTS_ARGS.to_vec();
    exists.push(&reference);
    let branch_exists = run_git(host, repo_root, &exists).is_ok();
    let worktree_exists = host.exists(worktree);
    if removal == VariantRemoval::OnlyIfSettled {
        if worktree_exists {
            ensure_settled(host, repo_root, worktree)?;
        }
        if branch_exists {
            ensure_branch_merged(host, repo_root, &reference)?;
        }
    }
    if worktree_exists {
        let target = git_path(host, repo_root, worktree);
        let mut args: Vec<&OsStr> = REMOVE_WORKTREE_ARGS.iter().map(OsStr::new).collect();
        if removal == VariantRemoval::Forced {
            args.push(OsStr::new(FORCE_FLAG));
        }
        args.push(target.as_os_str());
        run_git(host, repo_root, &args)?;
    }
    run_git(host, repo_root, &PRUNE_ARGS)?;
    if !branch_exists {
        return Ok(());
    }
    let mut delete: Vec<&str> = DELETE_BRANCH_ARGS.to_vec();
    delete.push(branch);
    run_git(host, repo_root, &delete)?;
    Ok(())
}

fn ensure_branch_merged(
    host: &HostLink,
    repo_root: &Path,
    reference: &str,
) -> Result<(), RepoError> {
    let mut args: Vec<&str> = COUNT_BRANCH_UNMERGED_ARGS.to_vec();
    args.extend([reference, NOT_FLAG, HEAD]);
    let counted = String::from_utf8_lossy(&run_git(host, repo_root, &args)?)
        .trim()
        .to_owned();
    let unmerged: usize = counted
        .parse()
        .map_err(|_| RepoError::UnexpectedOutput(COUNT_BRANCH_UNMERGED_ARGS.join(ARG_SEPARATOR)))?;
    if unmerged > 0 {
        return Err(RepoError::UnmergedCommits(unmerged));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_records_give_line_counts_and_binaries_count_none() {
        let records = [
            "3\t1\tsrc/a.rs",
            "-\t-\tlogo.png",
            "12\t0\tdocs/new file.md",
            "",
        ];
        let files = parse_numstat(&records.join("\0"));
        assert_eq!(
            files,
            [
                ChangedLines {
                    path: PathBuf::from("src/a.rs"),
                    added: 3,
                    removed: 1
                },
                ChangedLines {
                    path: PathBuf::from("logo.png"),
                    added: 0,
                    removed: 0
                },
                ChangedLines {
                    path: PathBuf::from("docs/new file.md"),
                    added: 12,
                    removed: 0
                },
            ]
        );
    }
}
