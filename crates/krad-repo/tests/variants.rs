use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_host::HostLink;
use krad_repo::{
    add_worktree, apply_variant, discard_variant, list_worktrees, variant_diff_summary, RepoError,
    VariantRemoval,
};

const HOST: HostLink = HostLink::Local;

type TestResult = Result<(), Box<dyn Error>>;

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
        .args(args)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into())
    }
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    let reference = format!("refs/heads/{branch}");
    git(repo, &["rev-parse", "--verify", "--quiet", &reference]).is_ok()
}

fn with_repo(label: &str, run: impl FnOnce(&Path) -> TestResult) -> TestResult {
    if !git_available() {
        return Ok(());
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base = std::env::temp_dir().join(format!(
        "krad-variants-{label}-{}-{nanos}",
        std::process::id()
    ));
    let repo = base.join("app");
    fs::create_dir_all(&repo)?;
    git(&repo, &["init", "--quiet", "--initial-branch=main"])?;
    git(&repo, &["config", "core.autocrlf", "false"])?;
    git(&repo, &["config", "user.name", "Test"])?;
    git(&repo, &["config", "user.email", "test@example.test"])?;
    fs::write(repo.join("lib.rs"), "fn one() {}\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "--quiet", "-m", "start"])?;
    let result = run(&repo);
    fs::remove_dir_all(&base)?;
    result
}

fn variant_with_changes(repo: &Path, name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let worktree = add_worktree(&HOST, repo, name)?;
    fs::write(worktree.path.join("lib.rs"), "fn one() {}\nfn two() {}\n")?;
    fs::write(worktree.path.join("notes.md"), "new\nfile\n")?;
    Ok(worktree.path)
}

#[test]
fn a_variant_summary_counts_committed_uncommitted_and_new_files() -> TestResult {
    with_repo("summary", |repo| {
        let variant = variant_with_changes(repo, "variant-1-1")?;
        fs::write(variant.join("done.txt"), "x\n")?;
        git(&variant, &["add", "done.txt"])?;
        git(&variant, &["commit", "--quiet", "-m", "part"])?;
        let summary = variant_diff_summary(&HOST, repo, &variant)?;
        let paths: Vec<&Path> = summary
            .files
            .iter()
            .map(|file| file.path.as_path())
            .collect();
        assert_eq!(
            paths,
            [
                Path::new("done.txt"),
                Path::new("lib.rs"),
                Path::new("notes.md")
            ]
        );
        assert_eq!((summary.added, summary.removed), (4, 0));
        Ok(())
    })
}

#[test]
fn the_winner_applies_onto_a_clean_base_as_uncommitted_changes() -> TestResult {
    with_repo("apply", |repo| {
        let variant = variant_with_changes(repo, "variant-1-2")?;
        let applied = apply_variant(&HOST, repo, &variant)?;
        assert_eq!(applied.file_count(), 2);
        assert_eq!(
            fs::read_to_string(repo.join("lib.rs"))?,
            "fn one() {}\nfn two() {}\n"
        );
        assert_eq!(fs::read_to_string(repo.join("notes.md"))?, "new\nfile\n");
        assert_eq!(git(repo, &["rev-list", "--count", "HEAD"])?, "1");
        Ok(())
    })
}

#[test]
fn a_base_with_changes_of_its_own_is_left_untouched() -> TestResult {
    with_repo("dirty", |repo| {
        let variant = variant_with_changes(repo, "variant-1-3")?;
        fs::write(repo.join("lib.rs"), "fn mine() {}\n")?;
        let refused = apply_variant(&HOST, repo, &variant);
        assert!(matches!(refused, Err(RepoError::Uncommitted(_))));
        assert_eq!(fs::read_to_string(repo.join("lib.rs"))?, "fn mine() {}\n");
        assert!(!repo.join("notes.md").exists());
        Ok(())
    })
}

#[test]
fn a_variant_that_no_longer_applies_leaves_the_base_untouched() -> TestResult {
    with_repo("conflict", |repo| {
        let variant = variant_with_changes(repo, "variant-1-4")?;
        fs::write(repo.join("lib.rs"), "fn other() {}\n")?;
        git(repo, &["commit", "--quiet", "-am", "moved on"])?;
        let refused = apply_variant(&HOST, repo, &variant);
        assert!(matches!(refused, Err(RepoError::Git { .. })));
        assert_eq!(fs::read_to_string(repo.join("lib.rs"))?, "fn other() {}\n");
        assert!(!repo.join("notes.md").exists());
        Ok(())
    })
}

#[test]
fn discarding_removes_the_worktree_and_branch_even_when_already_gone() -> TestResult {
    with_repo("discard", |repo| {
        let dirty = variant_with_changes(repo, "variant-2-1")?;
        let gone = variant_with_changes(repo, "variant-2-2")?;
        fs::remove_dir_all(&gone)?;
        discard_variant(&HOST, repo, &dirty, "variant-2-1", VariantRemoval::Forced)?;
        discard_variant(&HOST, repo, &gone, "variant-2-2", VariantRemoval::Forced)?;
        discard_variant(&HOST, repo, &gone, "variant-2-2", VariantRemoval::Forced)?;
        assert!(!dirty.exists());
        assert_eq!(list_worktrees(&HOST, repo)?.len(), 1);
        assert_eq!(git(repo, &["branch", "--list", "variant-*"])?, "");
        Ok(())
    })
}

#[test]
fn a_settled_discard_refuses_a_worktree_holding_uncommitted_work() -> TestResult {
    with_repo("refuse-dirty", |repo| {
        let dirty = variant_with_changes(repo, "variant-3-1")?;
        let refused = discard_variant(
            &HOST,
            repo,
            &dirty,
            "variant-3-1",
            VariantRemoval::OnlyIfSettled,
        );
        assert!(matches!(refused, Err(RepoError::UncommittedChanges)));
        assert!(dirty.join("notes.md").exists());
        assert!(branch_exists(repo, "variant-3-1"));
        Ok(())
    })
}

#[test]
fn a_settled_discard_refuses_a_branch_with_unmerged_commits_even_without_its_worktree() -> TestResult
{
    with_repo("refuse-unmerged", |repo| {
        let variant = variant_with_changes(repo, "variant-3-2")?;
        git(&variant, &["add", "."])?;
        git(&variant, &["commit", "--quiet", "-m", "work"])?;
        let refused = discard_variant(
            &HOST,
            repo,
            &variant,
            "variant-3-2",
            VariantRemoval::OnlyIfSettled,
        );
        assert!(matches!(refused, Err(RepoError::UnmergedCommits(1))));
        assert!(variant.exists());
        git(
            repo,
            &["worktree", "remove", "--force", &variant.to_string_lossy()],
        )?;
        let refused = discard_variant(
            &HOST,
            repo,
            &variant,
            "variant-3-2",
            VariantRemoval::OnlyIfSettled,
        );
        assert!(matches!(refused, Err(RepoError::UnmergedCommits(1))));
        assert!(branch_exists(repo, "variant-3-2"));
        Ok(())
    })
}

#[test]
fn a_settled_discard_removes_a_clean_worktree_and_its_branch() -> TestResult {
    with_repo("settled", |repo| {
        let fresh = add_worktree(&HOST, repo, "variant-3-3")?;
        discard_variant(
            &HOST,
            repo,
            &fresh.path,
            "variant-3-3",
            VariantRemoval::OnlyIfSettled,
        )?;
        assert!(!fresh.path.exists());
        assert_eq!(list_worktrees(&HOST, repo)?.len(), 1);
        assert_eq!(git(repo, &["branch", "--list", "variant-*"])?, "");
        Ok(())
    })
}
