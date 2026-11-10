use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_host::HostLink;
use krad_repo::{
    add_worktree, remove_settled_worktree, remove_worktree, repo_root, status, ChangeKind,
    FileChange, RepoError,
};

const WORKTREE_NAME: &str = "fix-login";
const SETTLED_NAME: &str = "close-worker";

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
