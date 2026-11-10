use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use krad_host::HostLink;
use krad_repo::{discard_folder, status, FileChange, RepoError};

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
        "krad-repo-folder-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(repo.join("src/deep"))?;
    git(&repo, &["init", "--quiet", "-b", "main"])?;
    git(&repo, &["config", "core.autocrlf", "false"])?;
    fs::write(repo.join(".gitignore"), "src/build.log\n")?;
    fs::write(repo.join("src/a.rs"), "one\n")?;
    fs::write(repo.join("src/gone.rs"), "gone\n")?;
    fs::write(repo.join("src/deep/old.rs"), "old\n")?;
    fs::write(repo.join("outside.rs"), "outside\n")?;
    fs::write(repo.join("src2.rs"), "beside\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "--quiet", "-m", "Start"])?;
    Ok(repo)
}

fn change_of<'a>(changes: &'a [FileChange], path: &str) -> Option<&'a FileChange> {
    changes.iter().find(|change| change.path == Path::new(path))
}

#[test]
fn a_folder_goes_back_to_head_and_nothing_outside_it_moves() -> Result<(), Box<dyn Error>> {
    let repo = repository("discard")?;
    let host = HostLink::Local;
    fs::write(repo.join("src/a.rs"), "changed\n")?;
    fs::remove_file(repo.join("src/gone.rs"))?;
    git(&repo, &["mv", "src/deep/old.rs", "src/deep/new.rs"])?;
    git(&repo, &["mv", "outside.rs", "src/moved_in.rs"])?;
    fs::write(repo.join("src/deep/fresh.rs"), "fresh\n")?;
    fs::write(repo.join("src/build.log"), "ignored\n")?;
    fs::write(repo.join("src2.rs"), "beside changed\n")?;
    fs::write(repo.join("loose.txt"), "loose\n")?;
    let held = discard_folder(&host, &repo, Path::new("src"))?;
    let left = status(&host, &repo)?.changes;
    let read = |path: &str| fs::read_to_string(repo.join(path));
    let restored = (
        read("src/a.rs")?,
        read("src/gone.rs")?,
        read("src/deep/old.rs")?,
    );
    let removed = ["src/deep/new.rs", "src/deep/fresh.rs", "src/moved_in.rs"]
        .map(|path| repo.join(path).exists());
    let ignored = read("src/build.log")?;
    let outside = read("outside.rs")?;
    let beside = read("src2.rs")?;
    let loose = repo.join("loose.txt").is_file();
    fs::remove_dir_all(&repo)?;
    assert_eq!(held.len(), 5, "{held:?}");
    assert_eq!(held.untracked, [PathBuf::from("src/deep/fresh.rs")]);
    assert_eq!(
        restored,
        ("one\n".to_owned(), "gone\n".to_owned(), "old\n".to_owned())
    );
    assert_eq!(removed, [false; 3]);
    assert_eq!(ignored, "ignored\n", "ignored files are never touched");
    assert_eq!(beside, "beside changed\n");
    assert!(loose);
    assert_eq!(
        change_of(&left, "outside.rs"),
        None,
        "a file moved in goes back to where it came from"
    );
    assert_eq!(outside, "outside\n");
    assert_eq!(left.len(), 2, "{left:?}");
    Ok(())
}

#[test]
fn a_file_moved_out_of_the_folder_goes_back_into_it_and_a_subfolder_repo_path_works(
) -> Result<(), Box<dyn Error>> {
    let repo = repository("moved-out")?;
    let host = HostLink::Local;
    fs::create_dir_all(repo.join("docs"))?;
    git(&repo, &["mv", "src/a.rs", "docs/a.rs"])?;
    let held = discard_folder(&host, &repo.join("src/deep"), Path::new("src"))?;
    let left = status(&host, &repo)?.changes;
    let back = fs::read_to_string(repo.join("src/a.rs"));
    let moved_gone = !repo.join("docs/a.rs").exists();
    fs::remove_dir_all(&repo)?;
    assert_eq!(held.crossing.len(), 1);
    assert_eq!(back?, "one\n");
    assert!(moved_gone);
    assert_eq!(left, []);
    Ok(())
}

#[test]
fn a_folder_of_only_new_files_is_emptied_of_them() -> Result<(), Box<dyn Error>> {
    let repo = repository("untracked")?;
    let host = HostLink::Local;
    fs::create_dir_all(repo.join("fresh/inner"))?;
    fs::write(repo.join("fresh/one.rs"), "one\n")?;
    fs::write(repo.join("fresh/inner/two.rs"), "two\n")?;
    let held = discard_folder(&host, &repo, Path::new("fresh"))?;
    let left = status(&host, &repo)?.changes;
    fs::remove_dir_all(&repo)?;
    assert_eq!(held.untracked.len(), 2);
    assert_eq!(held.tracked, Vec::<PathBuf>::new());
    assert_eq!(left, []);
    Ok(())
}

#[test]
fn a_folder_reaching_outside_the_repository_is_refused() -> Result<(), Box<dyn Error>> {
    let repo = repository("outside")?;
    let host = HostLink::Local;
    let refused = discard_folder(&host, &repo, Path::new("../"));
    let absolute = discard_folder(&host, &repo, &std::env::temp_dir());
    fs::remove_dir_all(&repo)?;
    assert!(matches!(refused, Err(RepoError::OutsideRepository(_))));
    assert!(matches!(absolute, Err(RepoError::OutsideRepository(_))));
    Ok(())
}
