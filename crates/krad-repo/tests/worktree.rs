use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_domain::{BranchPrefix, EnvFiles, WorktreeBase, WorktreeSettings, WorktreeState};
use krad_host::HostLink;
use krad_repo::{
    add_planned_worktree, add_worktree, remove_merged_worktree, remove_settled_worktree,
    remove_worktree, repo_root, status, worktree_state, Askpass, ChangeKind, FileChange, RepoError,
};

const WORKTREE_NAME: &str = "fix-login";
const SETTLED_NAME: &str = "close-worker";
const MERGED_NAME: &str = "auto-remove";
const PREFIX: &str = "krad/";

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git(repo: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into())
    }
}

fn unique_temp_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir =
        std::env::temp_dir().join(format!("krad-repo-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn exercise_repository(base: &Path) -> Result<(), Box<dyn Error>> {
    let repo = base.join("app");
    fs::create_dir(&repo)?;
    git(&repo, &["init", "--quiet"])?;
    fs::write(repo.join("README.md"), "hello\n")?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["commit", "--quiet", "-m", "initial"])?;
    fs::write(repo.join("notes.txt"), "draft\n")?;

    let root = repo_root(&HostLink::Local, &repo)?.ok_or("repository root not found")?;
    assert_eq!(fs::canonicalize(&root)?, fs::canonicalize(&repo)?);
    assert_eq!(repo_root(&HostLink::Local, base)?, None);

    let changes = status(&HostLink::Local, &root)?.changes;
    assert_eq!(
        changes,
        vec![FileChange {
            path: PathBuf::from("notes.txt"),
            kind: ChangeKind::Untracked,
            origin: None,
        }]
    );

    let added = add_worktree(&HostLink::Local, &root, WORKTREE_NAME)?;
    assert!(added.path.join("README.md").is_file());
    assert_eq!(added.branch.as_deref(), Some(WORKTREE_NAME));
    assert_eq!(
        git(&added.path, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim(),
        WORKTREE_NAME
    );
    let admin = root.join(".git").join("worktrees").join(WORKTREE_NAME);
    assert!(admin.is_dir());

    remove_worktree(&HostLink::Local, &root, &added.path)?;
    assert!(!added.path.exists());
    assert!(!admin.exists());
    Ok(())
}

#[test]
fn status_and_worktree_lifecycle_against_real_git() -> Result<(), Box<dyn Error>> {
    if !git_available() {
        return Err("git is not on PATH; this test needs the git CLI".into());
    }
    let base = unique_temp_dir(WORKTREE_NAME)?;
    let result = exercise_repository(&base);
    fs::remove_dir_all(&base)?;
    result
}

fn exercise_settled_removal(base: &Path) -> Result<(), Box<dyn Error>> {
    let repo = base.join("app");
    fs::create_dir(&repo)?;
    git(&repo, &["init", "--quiet"])?;
    fs::write(repo.join("README.md"), "hello\n")?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["commit", "--quiet", "-m", "initial"])?;
    let root = repo_root(&HostLink::Local, &repo)?.ok_or("repository root not found")?;
    let host = HostLink::Local;
    let added = add_worktree(&host, &root, SETTLED_NAME)?;

    fs::write(added.path.join("draft.txt"), "draft\n")?;
    let dirty = remove_settled_worktree(&host, &root, &added.path);
    assert!(
        matches!(dirty, Err(RepoError::UncommittedChanges)),
        "{dirty:?}"
    );
    assert!(added.path.join("draft.txt").is_file());

    git(&added.path, &["add", "draft.txt"])?;
    git(&added.path, &["commit", "--quiet", "-m", "draft"])?;
    let unmerged = remove_settled_worktree(&host, &root, &added.path);
    assert!(
        matches!(unmerged, Err(RepoError::UnmergedCommits(1))),
        "{unmerged:?}"
    );
    assert!(added.path.join("draft.txt").is_file());

    git(&root, &["merge", "--quiet", "--ff-only", SETTLED_NAME])?;
    remove_settled_worktree(&host, &root, &added.path)?;
    assert!(!added.path.exists());
    assert_eq!(
        git(&root, &["branch", "--list", SETTLED_NAME])?.trim(),
        SETTLED_NAME,
        "removing the worktree keeps its branch"
    );
    Ok(())
}

#[test]
fn a_worktree_with_work_left_is_never_removed() -> Result<(), Box<dyn Error>> {
    if !git_available() {
        return Err("git is not on PATH; this test needs the git CLI".into());
    }
    let base = unique_temp_dir(SETTLED_NAME)?;
    let result = exercise_settled_removal(&base);
    fs::remove_dir_all(&base)?;
    result
}

fn exercise_merged_removal(base: &Path) -> Result<(), Box<dyn Error>> {
    let repo = base.join("app");
    fs::create_dir(&repo)?;
    git(&repo, &["init", "--quiet"])?;
    fs::write(repo.join("README.md"), "hello\n")?;
    fs::write(repo.join(".gitignore"), ".env*\n")?;
    fs::write(repo.join(".env.local"), "TOKEN=1\n")?;
    git(&repo, &["add", "README.md", ".gitignore"])?;
    git(&repo, &["commit", "--quiet", "-m", "initial"])?;
    let root = repo_root(&HostLink::Local, &repo)?.ok_or("repository root not found")?;
    let host = HostLink::Local;
    let settings = WorktreeSettings {
        branch_prefix: BranchPrefix::from(PREFIX),
        env_files: EnvFiles::Copy,
        ..WorktreeSettings::default()
    };
    let planned = add_planned_worktree(&host, &root, MERGED_NAME, &settings, &Askpass::default())?;
    assert!(planned.setbacks.is_empty(), "{:?}", planned.setbacks);
    let added = planned.worktree;
    let branch = format!("{PREFIX}{MERGED_NAME}");
    assert_eq!(added.branch.as_deref(), Some(branch.as_str()));
    assert_eq!(
        fs::read_to_string(added.path.join(".env.local"))?,
        "TOKEN=1\n"
    );
    let state =
        |path: &Path| worktree_state(&host, &root, path, Some(&branch), WorktreeBase::CurrentHead);
    assert_eq!(state(&added.path)?, WorktreeState::Fresh);
    assert_eq!(
        remove_merged_worktree(
            &host,
            &root,
            &added.path,
            Some(&branch),
            WorktreeBase::CurrentHead
        )?,
        WorktreeState::Fresh
    );
    assert!(added.path.is_dir(), "an untouched worktree stays");

    fs::write(added.path.join("draft.txt"), "draft\n")?;
    assert_eq!(
        remove_merged_worktree(
            &host,
            &root,
            &added.path,
            Some(&branch),
            WorktreeBase::CurrentHead
        )?,
        WorktreeState::Dirty
    );
    git(&added.path, &["add", "draft.txt"])?;
    git(&added.path, &["commit", "--quiet", "-m", "draft"])?;
    assert_eq!(
        remove_merged_worktree(
            &host,
            &root,
            &added.path,
            Some(&branch),
            WorktreeBase::CurrentHead
        )?,
        WorktreeState::Ahead(1)
    );
    assert!(added.path.join("draft.txt").is_file());

    git(&root, &["merge", "--quiet", "--ff-only", &branch])?;
    assert_eq!(
        remove_merged_worktree(
            &host,
            &root,
            &added.path,
            Some(&branch),
            WorktreeBase::CurrentHead
        )?,
        WorktreeState::Merged
    );
    assert!(!added.path.exists());
    Ok(())
}

#[test]
fn only_a_merged_and_clean_worktree_is_removed_automatically() -> Result<(), Box<dyn Error>> {
    if !git_available() {
        return Err("git is not on PATH; this test needs the git CLI".into());
    }
    let base = unique_temp_dir(MERGED_NAME)?;
    let result = exercise_merged_removal(&base);
    fs::remove_dir_all(&base)?;
    result
}
