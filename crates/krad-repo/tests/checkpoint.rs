use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_host::HostLink;
use krad_repo::{
    checkpoint_changes, restore_checkpoint, snapshot_checkpoint, CheckpointChange,
    CheckpointChangeKind,
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

fn temp_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "krad-checkpoint-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn new_repo(path: &Path) -> TestResult {
    git(path, &["init", "--quiet", "--initial-branch=main"])?;
    git(path, &["config", "core.autocrlf", "false"])?;
    git(path, &["config", "user.name", "Test"])?;
    git(path, &["config", "user.email", "test@example.com"])?;
    git(path, &["config", "commit.gpgsign", "false"])?;
    git(path, &["config", "core.autocrlf", "false"])?;
    Ok(())
}

fn with_repo(label: &str, commit: bool, run: impl FnOnce(&Path) -> TestResult) -> TestResult {
    if !git_available() {
        return Ok(());
    }
    let repo = temp_dir(label)?;
    new_repo(&repo)?;
    fs::write(repo.join(".gitignore"), "target/\n")?;
    fs::write(repo.join("README.md"), "hello\n")?;
    fs::create_dir_all(repo.join("src"))?;
    fs::write(repo.join("src/lib.rs"), "fn a() {}\n")?;
    if commit {
        git(&repo, &["add", "--all"])?;
        git(&repo, &["commit", "--quiet", "-m", "initial"])?;
    }
    let result = run(&repo);
    fs::remove_dir_all(&repo)?;
    result
}

fn read(repo: &Path, path: &str) -> Result<String, Box<dyn Error>> {
    Ok(fs::read_to_string(repo.join(path))?)
}

struct Untouched {
    head: String,
    refs: String,
    staged: String,
    stash: String,
}

fn untouched(repo: &Path) -> Result<Untouched, Box<dyn Error>> {
    Ok(Untouched {
        head: git(repo, &["rev-parse", "--verify", "--quiet", "HEAD"]).unwrap_or_default(),
        refs: git(repo, &["for-each-ref"])?,
        staged: git(repo, &["diff", "--cached", "--name-status"]).unwrap_or_default(),
        stash: git(repo, &["stash", "list"])?,
    })
}

fn assert_untouched(repo: &Path, before: &Untouched) -> TestResult {
    let after = untouched(repo)?;
    assert_eq!(after.head, before.head, "HEAD stays where it was");
    assert_eq!(after.refs, before.refs, "no branch, tag or ref is written");
    assert_eq!(
        after.staged, before.staged,
        "the index keeps what was staged"
    );
    assert_eq!(after.stash, before.stash, "the stash is not used");
    let leftovers: Vec<_> = fs::read_dir(repo.join(".git"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("krad-"))
        .collect();
    assert!(leftovers.is_empty(), "no scratch index is left behind");
    Ok(())
}

#[test]
fn a_snapshot_records_the_working_tree_without_touching_index_refs_or_head() -> TestResult {
    with_repo("snapshot", true, |repo| {
        fs::write(repo.join("README.md"), "staged edit\n")?;
        git(repo, &["add", "README.md"])?;
        fs::write(repo.join("README.md"), "worktree edit\n")?;
        fs::write(repo.join("notes.txt"), "untracked\n")?;
        fs::create_dir_all(repo.join("target"))?;
        fs::write(repo.join("target/out.bin"), "ignored\n")?;
        let before = untouched(repo)?;

        let checkpoint = snapshot_checkpoint(&HOST, repo)?;

        assert_untouched(repo, &before)?;
        let files = git(repo, &["ls-tree", "-r", "--name-only", checkpoint.as_str()])?;
        assert_eq!(
            files.lines().collect::<Vec<_>>(),
            [".gitignore", "README.md", "notes.txt", "src/lib.rs"],
            "untracked files are kept and ignored ones are not"
        );
        let readme = format!("{}:README.md", checkpoint.as_str());
        assert_eq!(git(repo, &["show", &readme])?, "worktree edit");
        assert_eq!(snapshot_checkpoint(&HOST, repo)?, checkpoint);
        Ok(())
    })
}

#[test]
fn a_restore_brings_the_files_back_and_lists_them_first() -> TestResult {
    with_repo("restore", true, |repo| {
        fs::write(repo.join("notes.txt"), "turn one\n")?;
        let turn = snapshot_checkpoint(&HOST, repo)?;

        fs::write(repo.join("src/lib.rs"), "fn b() {}\n")?;
        fs::remove_file(repo.join("notes.txt"))?;
        fs::write(repo.join("src/added.rs"), "new\n")?;
        fs::create_dir_all(repo.join("target"))?;
        fs::write(repo.join("target/out.bin"), "ignored\n")?;
        let before = untouched(repo)?;

        let expected = vec![
            CheckpointChange {
                path: PathBuf::from("notes.txt"),
                kind: CheckpointChangeKind::Recreated,
            },
            CheckpointChange {
                path: PathBuf::from("src/added.rs"),
                kind: CheckpointChangeKind::Removed,
            },
            CheckpointChange {
                path: PathBuf::from("src/lib.rs"),
                kind: CheckpointChangeKind::Reverted,
            },
        ];
        assert_eq!(checkpoint_changes(&HOST, repo, &turn)?, expected);

        let restored = restore_checkpoint(&HOST, repo, &turn)?;

        assert_eq!(restored.changes, expected);
        assert_eq!(read(repo, "notes.txt")?, "turn one\n");
        assert_eq!(read(repo, "src/lib.rs")?, "fn a() {}\n");
        assert!(!repo.join("src/added.rs").exists());
        assert_eq!(
            read(repo, "target/out.bin")?,
            "ignored\n",
            "ignored files stay"
        );
        assert_untouched(repo, &before)?;
        assert_eq!(checkpoint_changes(&HOST, repo, &turn)?, Vec::new());
        Ok(())
    })
}

#[test]
fn undoing_a_restore_gives_back_the_uncommitted_work() -> TestResult {
    with_repo("undo", true, |repo| {
        let turn = snapshot_checkpoint(&HOST, repo)?;
        fs::write(repo.join("README.md"), "my own edit\n")?;
        fs::write(repo.join("draft.txt"), "never committed\n")?;
        fs::remove_file(repo.join("src/lib.rs"))?;

        let restored = restore_checkpoint(&HOST, repo, &turn)?;
        assert_eq!(read(repo, "README.md")?, "hello\n");
        assert!(!repo.join("draft.txt").exists());

        let undone = restore_checkpoint(&HOST, repo, &restored.undo)?;

        assert_eq!(read(repo, "README.md")?, "my own edit\n");
        assert_eq!(read(repo, "draft.txt")?, "never committed\n");
        assert!(!repo.join("src/lib.rs").exists());
        assert_eq!(undone.undo, turn, "the undo of an undo is the restore");
        Ok(())
    })
}

#[test]
fn checkpoints_work_in_a_repository_with_no_commit_yet() -> TestResult {
    with_repo("unborn", false, |repo| {
        let before = untouched(repo)?;
        let turn = snapshot_checkpoint(&HOST, repo)?;
        fs::write(repo.join("README.md"), "changed\n")?;
        fs::write(repo.join("extra.txt"), "extra\n")?;

        let restored = restore_checkpoint(&HOST, repo, &turn)?;

        assert_eq!(read(repo, "README.md")?, "hello\n");
        assert!(!repo.join("extra.txt").exists());
        assert_eq!(restored.changes.len(), 2);
        assert_untouched(repo, &before)?;
        Ok(())
    })
}

#[test]
fn a_checkpoint_taken_from_a_subfolder_covers_the_whole_repository() -> TestResult {
    with_repo("subfolder", true, |repo| {
        let src = repo.join("src");
        let turn = snapshot_checkpoint(&HOST, &src)?;
        fs::write(repo.join("README.md"), "changed\n")?;
        fs::write(src.join("lib.rs"), "changed\n")?;

        assert_eq!(restore_checkpoint(&HOST, &src, &turn)?.changes.len(), 2);

        assert_eq!(read(repo, "README.md")?, "hello\n");
        assert_eq!(read(repo, "src/lib.rs")?, "fn a() {}\n");
        Ok(())
    })
}

#[test]
fn a_folder_outside_git_has_no_checkpoint() -> TestResult {
    if !git_available() {
        return Ok(());
    }
    let dir = temp_dir("plain")?;
    let result = snapshot_checkpoint(&HOST, &dir);
    fs::remove_dir_all(&dir)?;
    assert!(result.is_err());
    Ok(())
}
