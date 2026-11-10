use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_host::HostLink;
use krad_repo::{
    revert_session_file, revert_session_hunk, session_diff, snapshot_tree, FilePatch, PatchStatus,
    RepoError,
};

const HOST: HostLink = HostLink::Local;
const ORIGINAL: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\n";

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

fn with_repo(label: &str, run: impl FnOnce(&Path) -> TestResult) -> TestResult {
    if !git_available() {
        return Ok(());
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base = std::env::temp_dir().join(format!(
        "krad-session-{label}-{}-{nanos}",
        std::process::id()
    ));
    let repo = base.join("app");
    fs::create_dir_all(&repo)?;
    git(&repo, &["init", "--quiet", "--initial-branch=main"])?;
    git(&repo, &["config", "core.autocrlf", "false"])?;
    git(&repo, &["config", "user.name", "Test"])?;
    git(&repo, &["config", "user.email", "test@example.com"])?;
    git(&repo, &["config", "commit.gpgsign", "false"])?;
    git(&repo, &["config", "core.autocrlf", "false"])?;
    fs::write(repo.join("story.txt"), ORIGINAL)?;
    fs::write(repo.join("other.txt"), "other\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "--quiet", "-m", "initial"])?;
    let result = run(&repo);
    fs::remove_dir_all(&base)?;
    result
}

fn patch_of<'a>(files: &'a [FilePatch], path: &str) -> Result<&'a FilePatch, Box<dyn Error>> {
    files
        .iter()
        .find(|file| file.path == Path::new(path))
        .ok_or_else(|| format!("{path} is not in the session diff").into())
}

fn paths(files: &[FilePatch]) -> Vec<PathBuf> {
    files.iter().map(|file| file.path.clone()).collect()
}

#[test]
fn the_session_diff_shows_only_what_changed_after_the_baseline() -> TestResult {
    with_repo("baseline", |repo| {
        fs::write(repo.join("other.txt"), "edited by the user before\n")?;
        let baseline = snapshot_tree(&HOST, repo)?;
        let index_before = git(repo, &["diff", "--cached", "--name-only"])?;
        fs::write(repo.join("story.txt"), ORIGINAL.replace("two", "TWO"))?;
        fs::write(repo.join("fresh.txt"), "new\n")?;
        let diff = session_diff(&HOST, repo, &baseline)?;
        assert_eq!(
            paths(&diff.files),
            [PathBuf::from("fresh.txt"), PathBuf::from("story.txt")]
        );
        assert_eq!(
            patch_of(&diff.files, "fresh.txt")?.status,
            PatchStatus::Added
        );
        assert_eq!(
            git(repo, &["diff", "--cached", "--name-only"])?,
            index_before
        );
        let leftovers = fs::read_dir(repo.join(".git"))?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("krad-session"))
            .count();
        assert_eq!(leftovers, 0);
        assert_eq!(
            git(repo, &["status", "--porcelain", "--", "fresh.txt"])?,
            "?? fresh.txt"
        );
        Ok(())
    })
}

#[test]
fn reverting_a_file_restores_its_baseline_and_keeps_the_users_earlier_edits() -> TestResult {
    with_repo("file", |repo| {
        let before_session = ORIGINAL.replace("eleven", "ELEVEN by the user");
        fs::write(repo.join("story.txt"), &before_session)?;
        let baseline = snapshot_tree(&HOST, repo)?;
        fs::write(repo.join("story.txt"), before_session.replace("two", "TWO"))?;
        fs::write(repo.join("other.txt"), "agent\n")?;
        fs::write(repo.join("fresh.txt"), "new\n")?;
        fs::remove_file(repo.join("other.txt"))?;
        let diff = session_diff(&HOST, repo, &baseline)?;
        revert_session_file(&HOST, repo, patch_of(&diff.files, "story.txt")?)?;
        assert_eq!(fs::read_to_string(repo.join("story.txt"))?, before_session);
        assert!(repo.join("fresh.txt").exists());
        revert_session_file(&HOST, repo, patch_of(&diff.files, "fresh.txt")?)?;
        assert!(!repo.join("fresh.txt").exists());
        revert_session_file(&HOST, repo, patch_of(&diff.files, "other.txt")?)?;
        assert_eq!(fs::read_to_string(repo.join("other.txt"))?, "other\n");
        assert_eq!(session_diff(&HOST, repo, &baseline)?.files, []);
        Ok(())
    })
}

#[test]
fn reverting_a_hunk_touches_only_that_hunk() -> TestResult {
    with_repo("hunk", |repo| {
        let baseline = snapshot_tree(&HOST, repo)?;
        let agent = ORIGINAL.replace("two", "TWO").replace("ten", "TEN");
        fs::write(repo.join("story.txt"), &agent)?;
        let diff = session_diff(&HOST, repo, &baseline)?;
        let story = patch_of(&diff.files, "story.txt")?;
        assert_eq!(story.hunks().len(), 2);
        fs::write(
            repo.join("story.txt"),
            agent.replace("five", "FIVE by the user"),
        )?;
        revert_session_hunk(&HOST, repo, story, 1)?;
        assert_eq!(
            fs::read_to_string(repo.join("story.txt"))?,
            ORIGINAL
                .replace("two", "TWO")
                .replace("five", "FIVE by the user")
        );
        Ok(())
    })
}

#[test]
fn a_file_or_hunk_edited_after_its_diff_was_read_is_left_alone() -> TestResult {
    with_repo("stale", |repo| {
        let baseline = snapshot_tree(&HOST, repo)?;
        let agent = ORIGINAL.replace("two", "TWO").replace("ten", "TEN");
        fs::write(repo.join("story.txt"), &agent)?;
        let diff = session_diff(&HOST, repo, &baseline)?;
        let story = patch_of(&diff.files, "story.txt")?;
        let edited = agent.replace("TEN", "TEN and more");
        fs::write(repo.join("story.txt"), &edited)?;
        assert!(matches!(
            revert_session_hunk(&HOST, repo, story, 1),
            Err(RepoError::Stale(_))
        ));
        assert!(matches!(
            revert_session_file(&HOST, repo, story),
            Err(RepoError::Stale(_))
        ));
        assert_eq!(fs::read_to_string(repo.join("story.txt"))?, edited);
        Ok(())
    })
}
