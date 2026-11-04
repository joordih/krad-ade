use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_domain::CommitLabel;
use krad_host::HostLink;
use krad_repo::{
    commit_diff, commit_files, commit_log, discard_change, status, ChangeKind, LineCount,
};

fn git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com"])
        .args(args)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into())
    }
}

fn repository(label: &str) -> Result<PathBuf, Box<dyn Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let repo = std::env::temp_dir().join(format!(
        "krad-repo-history-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&repo)?;
    git(&repo, &["init", "--quiet", "-b", "main"])?;
    fs::write(repo.join("a.txt"), "one\ntwo\n")?;
    fs::write(repo.join("old.txt"), "kept\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "--quiet", "-m", "Start"])?;
    Ok(repo)
}

#[test]
fn every_kind_of_change_is_discarded_back_to_head() -> Result<(), Box<dyn Error>> {
    let repo = repository("discard")?;
    let host = HostLink::Local;
    fs::write(repo.join("a.txt"), "changed\n")?;
    fs::write(repo.join("new.txt"), "fresh\n")?;
    fs::write(repo.join("staged.txt"), "staged\n")?;
    git(&repo, &["add", "staged.txt"])?;
    git(&repo, &["mv", "old.txt", "moved.txt"])?;
    for change in status(&host, &repo)?.changes {
        discard_change(&host, &repo, &change)?;
    }
    let left = status(&host, &repo)?.changes;
    let text = fs::read_to_string(repo.join("a.txt"))?;
    let restored = repo.join("old.txt").is_file();
    let removed = !repo.join("new.txt").exists() && !repo.join("staged.txt").exists();
    fs::remove_dir_all(&repo)?;
    assert_eq!(left, []);
    assert_eq!(text, "one\ntwo\n");
    assert!(restored && removed);
    Ok(())
}

#[test]
fn the_log_lists_commits_with_their_files_and_diffs() -> Result<(), Box<dyn Error>> {
    let repo = repository("log")?;
    let host = HostLink::Local;
    fs::write(repo.join("a.txt"), "one\nthree\n")?;
    git(&repo, &["mv", "old.txt", "moved.txt"])?;
    git(&repo, &["commit", "--quiet", "-am", "Second"])?;
    git(&repo, &["tag", "v1"])?;
    let log = commit_log(&host, &repo, &["HEAD"], 10)?;
    let files = log
        .first()
        .map(|commit| commit_files(&host, &repo, &commit.sha))
        .transpose()?
        .unwrap_or_default();
    let diff = match (log.first(), files.first()) {
        (Some(commit), Some(file)) => commit_diff(&host, &repo, &commit.sha, &file.change)?,
        _ => String::new(),
    };
    let root_files = log
        .last()
        .map(|commit| commit_files(&host, &repo, &commit.sha))
        .transpose()?
        .unwrap_or_default();
    fs::remove_dir_all(&repo)?;

    let subjects: Vec<&str> = log.iter().map(|commit| commit.subject.as_str()).collect();
    assert_eq!(subjects, ["Second", "Start"]);
    assert_eq!(
        log.first().map(|commit| commit.author.as_str()),
        Some("Ada")
    );
    assert!(log
        .first()
        .is_some_and(|commit| commit.labels.contains(&CommitLabel::Tag("v1".to_owned()))));
    let kinds: Vec<(ChangeKind, LineCount)> = files
        .iter()
        .map(|file| (file.change.kind, file.lines))
        .collect();
    assert_eq!(
        kinds,
        [
            (
                ChangeKind::Modified,
                LineCount {
                    added: 1,
                    removed: 1
                }
            ),
            (ChangeKind::Renamed, LineCount::default()),
        ]
    );
    assert_eq!(diff, "@@ -1,2 +1,2 @@\n one\n-two\n+three\n");
    assert_eq!(root_files.len(), 2);
    Ok(())
}
