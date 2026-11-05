use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_host::HostLink;
use krad_repo::{
    add_branch_worktree, check_out, commit, create_branch, create_tag, delete_tag, fetch,
    list_refs, pop_stash, pull, push, push_tags, stage, stash_changes, status, sync, unstage,
    Askpass, BranchPlace, ChangeKind, CommitMode, CommitScope, PushTarget,
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
    let dir =
        std::env::temp_dir().join(format!("krad-repo-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn new_repo(path: &Path) -> TestResult {
    fs::create_dir_all(path)?;
    git(path, &["init", "--quiet", "--initial-branch=main"])?;
    git(path, &["config", "user.name", "Test"])?;
    git(path, &["config", "user.email", "test@example.com"])?;
    git(path, &["config", "commit.gpgsign", "false"])?;
    Ok(())
}

fn with_repo(label: &str, run: impl FnOnce(&Path) -> TestResult) -> TestResult {
    if !git_available() {
        return Ok(());
    }
    let base = temp_dir(label)?;
    let repo = base.join("app");
    new_repo(&repo)?;
    fs::write(repo.join("README.md"), "hello\n")?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["commit", "--quiet", "-m", "initial"])?;
    let result = run(&repo);
    fs::remove_dir_all(&base)?;
    result
}

#[test]
fn staging_moves_a_file_between_the_sides_and_commits_take_only_what_is_staged() -> TestResult {
    with_repo("stage", |repo| {
        fs::write(repo.join("README.md"), "hello again\n")?;
        fs::write(repo.join("notes.txt"), "draft\n")?;
        stage(&HOST, repo, &[PathBuf::from("notes.txt")])?;
        let staged = status(&HOST, repo)?;
        assert_eq!(staged.staged.len(), 1);
        assert_eq!(staged.staged[0].kind, ChangeKind::Added);
        assert_eq!(staged.unstaged.len(), 1);

        unstage(&HOST, repo, &[PathBuf::from("notes.txt")])?;
        assert_eq!(status(&HOST, repo)?.staged.len(), 0);

        stage(&HOST, repo, &[PathBuf::from("notes.txt")])?;
        commit(
            &HOST,
            repo,
            "add notes",
            CommitMode::New,
            CommitScope::Staged,
        )?;
        let after = status(&HOST, repo)?;
        assert_eq!(after.staged.len(), 0);
        assert_eq!(after.unstaged.len(), 1, "the unstaged edit stays behind");
        assert_eq!(git(repo, &["log", "-1", "--format=%s"])?, "add notes");

        commit(&HOST, repo, "", CommitMode::Amend, CommitScope::Everything)?;
        assert_eq!(status(&HOST, repo)?.changes.len(), 0);
        assert_eq!(git(repo, &["log", "-1", "--format=%s"])?, "add notes");
        assert_eq!(git(repo, &["rev-list", "--count", "HEAD"])?, "2");
        Ok(())
    })
}

#[test]
fn a_commit_with_nothing_staged_can_take_every_change() -> TestResult {
    with_repo("commit-all", |repo| {
        fs::write(repo.join("new.txt"), "new\n")?;
        commit(
            &HOST,
            repo,
            "everything",
            CommitMode::New,
            CommitScope::Everything,
        )?;
        assert_eq!(status(&HOST, repo)?.changes.len(), 0);
        Ok(())
    })
}

#[test]
fn a_stash_sets_changes_aside_and_pops_them_back() -> TestResult {
    with_repo("stash", |repo| {
        fs::write(repo.join("README.md"), "changed\n")?;
        fs::write(repo.join("untracked.txt"), "new\n")?;
        stash_changes(&HOST, repo)?;
        assert_eq!(status(&HOST, repo)?.changes.len(), 0);
        pop_stash(&HOST, repo)?;
        assert_eq!(status(&HOST, repo)?.changes.len(), 2);
        assert!(pop_stash(&HOST, repo).is_err(), "no stash is left to pop");
        Ok(())
    })
}

#[test]
fn branches_and_tags_are_listed_created_checked_out_and_deleted() -> TestResult {
    with_repo("branches", |repo| {
        create_branch(&HOST, repo, "feature")?;
        assert_eq!(status(&HOST, repo)?.branch.as_deref(), Some("feature"));
        let refs = list_refs(&HOST, repo)?;
        let main = refs
            .local()
            .find(|branch| branch.name == "main")
            .ok_or("main is listed")?
            .clone();
        assert!(!main.is_head);
        assert!(refs
            .local()
            .any(|branch| branch.name == "feature" && branch.is_head));
        check_out(&HOST, repo, &main)?;
        assert_eq!(status(&HOST, repo)?.branch.as_deref(), Some("main"));
        let feature = refs
            .local()
            .find(|branch| branch.name == "feature")
            .ok_or("feature is listed")?
            .clone();
        let worktree = add_branch_worktree(&HOST, repo, &feature)?;
        assert_eq!(
            status(&HOST, &worktree.path)?.branch.as_deref(),
            Some("feature")
        );
        assert!(
            check_out(&HOST, repo, &feature).is_err(),
            "a worktree holds it"
        );

        create_tag(&HOST, repo, "v1.0")?;
        assert_eq!(list_refs(&HOST, repo)?.tags, ["v1.0"]);
        delete_tag(&HOST, repo, "v1.0")?;
        assert_eq!(list_refs(&HOST, repo)?.tags.len(), 0);
        assert!(create_branch(&HOST, repo, "bad name").is_err());
        Ok(())
    })
}

#[test]
fn a_branch_is_published_pulled_and_a_remote_one_checked_out_as_a_tracking_branch() -> TestResult {
    with_repo("remote", |repo| {
        let base = repo.parent().ok_or("repo has a parent")?;
        let remote = base.join("remote.git");
        git(base, &["init", "--quiet", "--bare", "remote.git"])?;
        git(
            repo,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )?;
        let askpass = Askpass::default();
        let publish = PushTarget::Publish {
            remote: "origin".to_owned(),
            branch: "main".to_owned(),
        };
        push(&HOST, repo, &publish, &askpass)?;
        assert_eq!(
            status(&HOST, repo)?.upstream.as_deref(),
            Some("origin/main")
        );

        let other = base.join("other");
        git(
            base,
            &["clone", "--quiet", &remote.to_string_lossy(), "other"],
        )?;
        git(&other, &["config", "user.name", "Test"])?;
        git(&other, &["config", "user.email", "test@example.com"])?;
        git(&other, &["switch", "--quiet", "-c", "topic"])?;
        fs::write(other.join("topic.txt"), "topic\n")?;
        git(&other, &["add", "topic.txt"])?;
        git(&other, &["commit", "--quiet", "-m", "topic"])?;
        git(&other, &["push", "--quiet", "origin", "topic"])?;
        git(&other, &["switch", "--quiet", "main"])?;
        fs::write(other.join("README.md"), "from elsewhere\n")?;
        git(&other, &["commit", "--quiet", "-am", "elsewhere"])?;
        git(&other, &["push", "--quiet", "origin", "main"])?;

        fetch(&HOST, repo, &askpass)?;
        let refs = list_refs(&HOST, repo)?;
        let topic = refs
            .remote()
            .find(|branch| branch.name == "topic")
            .ok_or("the fetched branch is listed")?
            .clone();
        assert_eq!(topic.place, BranchPlace::Remote("origin".to_owned()));
        assert_eq!(status(&HOST, repo)?.behind, 1);

        pull(&HOST, repo, &askpass)?;
        assert_eq!(
            fs::read_to_string(repo.join("README.md"))?,
            "from elsewhere\n"
        );

        fs::write(repo.join("local.txt"), "local\n")?;
        commit(
            &HOST,
            repo,
            "local",
            CommitMode::New,
            CommitScope::Everything,
        )?;
        create_tag(&HOST, repo, "v2")?;
        sync(&HOST, repo, &PushTarget::Upstream, &askpass)?;
        push_tags(&HOST, repo, &askpass)?;
        assert_eq!(status(&HOST, repo)?.ahead, 0);
        assert_eq!(git(&remote, &["tag"])?, "v2");

        check_out(&HOST, repo, &topic)?;
        let switched = status(&HOST, repo)?;
        assert_eq!(switched.branch.as_deref(), Some("topic"));
        assert_eq!(switched.upstream.as_deref(), Some("origin/topic"));
        git(repo, &["switch", "--quiet", "main"])?;
        check_out(&HOST, repo, &topic)?;
        assert_eq!(status(&HOST, repo)?.branch.as_deref(), Some("topic"));
        Ok(())
    })
}
