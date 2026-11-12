use krad_host::HostLink;
use std::path::{Path, PathBuf};

use super::{HEAD, MAX_UNTRACKED_BYTES};
use crate::git::run_git;
use crate::process::git_output;
use crate::RepoError;

const TRACKED_PATCH_ARGS: [&str; 6] = [
    "diff",
    "--no-color",
    "--no-ext-diff",
    "--find-renames",
    "--binary",
    HEAD,
];
const UNTRACKED_ARGS: [&str; 4] = ["ls-files", "--others", "--exclude-standard", "-z"];
const NEW_FILE_ARGS: [&str; 7] = [
    "diff",
    "--no-color",
    "--no-ext-diff",
    "--binary",
    "--no-index",
    "--",
    "/dev/null",
];
const RECORD_SEPARATOR: u8 = 0;
const DIFFERS: i32 = 1;

fn untracked_files(host: &HostLink, repo: &Path) -> Result<Vec<PathBuf>, RepoError> {
    let listed = run_git(host, repo, &UNTRACKED_ARGS)?;
    Ok(listed
        .split(|&byte| byte == RECORD_SEPARATOR)
        .filter(|record| !record.is_empty())
        .map(|record| PathBuf::from(String::from_utf8_lossy(record).into_owned()))
        .collect())
}

fn new_file_patch(host: &HostLink, repo: &Path, file: &Path) -> Result<String, RepoError> {
    let mut args: Vec<&std::ffi::OsStr> = NEW_FILE_ARGS.iter().map(std::ffi::OsStr::new).collect();
    args.push(file.as_os_str());
    let output = git_output(host, repo, &args)?;
    match output.status.code() {
        Some(0 | DIFFERS) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        _ => Err(RepoError::Git {
            command: format!("git diff --no-index {}", file.display()),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        }),
    }
}

fn small_enough(host: &HostLink, repo: &Path, file: &Path) -> bool {
    host.metadata(&repo.join(file))
        .is_ok_and(|metadata| metadata.len <= MAX_UNTRACKED_BYTES)
}

/// # Errors
///
/// Returns [`RepoError::Git`] when `git diff` or `git ls-files` fails, as in
/// a repository with no commit yet.
pub fn working_patch_with_untracked(host: &HostLink, repo: &Path) -> Result<String, RepoError> {
    let mut patch =
        String::from_utf8_lossy(&run_git(host, repo, &TRACKED_PATCH_ARGS)?).into_owned();
    for file in untracked_files(host, repo)?
        .into_iter()
        .filter(|file| small_enough(host, repo, file))
    {
        patch.push_str(&new_file_patch(host, repo, &file)?);
    }
    Ok(patch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        let status = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
            .arg(dir)
            .args(args)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git {args:?} failed").into())
        }
    }

    #[test]
    fn the_patch_carries_new_files_but_not_ignored_ones() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = std::env::temp_dir().join(format!("krad-repo-untracked-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        git(&dir, &["init", "--quiet"])?;
        std::fs::write(dir.join("kept.txt"), "one\n")?;
        std::fs::write(dir.join(".gitignore"), "build/\n")?;
        git(&dir, &["add", "."])?;
        git(&dir, &["commit", "--quiet", "-m", "start"])?;
        std::fs::write(dir.join("kept.txt"), "two\n")?;
        std::fs::create_dir_all(dir.join("notes"))?;
        std::fs::write(dir.join("notes/new.txt"), "fresh\n")?;
        std::fs::create_dir_all(dir.join("build"))?;
        std::fs::write(dir.join("build/out.txt"), "ignored\n")?;
        let patch = working_patch_with_untracked(&HostLink::Local, &dir)?;
        assert!(patch.contains("+two"));
        assert!(patch.contains("+++ b/notes/new.txt"));
        assert!(patch.contains("+fresh"));
        assert!(!patch.contains("ignored"));
        git(&dir, &["diff", "--cached", "--quiet"])?;
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
