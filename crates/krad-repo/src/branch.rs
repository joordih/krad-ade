use krad_domain::review::resolve_ssh_alias;
use std::path::Path;

use crate::git::run_git;
use crate::RepoError;

const SSH_DIR: &str = ".ssh";
const SSH_CONFIG: &str = "config";
const NO_SUCH_REMOTE: &str = "No such remote";
const DIFF_ARGS: [&str; 4] = ["diff", "--no-color", "--no-ext-diff", "--find-renames"];
const PATHSPEC_SEPARATOR: &str = "--";
const HUNK_PREFIX: &str = "@@";

pub const ORIGIN_REMOTE: &str = "origin";

/// # Errors
///
/// Returns [`RepoError::Git`] when `repo` is not a git repository.
pub fn remote_url(repo: &Path, remote: &str) -> Result<Option<String>, RepoError> {
    match run_git(repo, &["remote", "get-url", remote]) {
        Ok(stdout) => Ok(Some(String::from_utf8_lossy(&stdout).trim().to_owned())),
        Err(RepoError::Git { message, .. }) if message.contains(NO_SUCH_REMOTE) => Ok(None),
        Err(error) => Err(error),
    }
}

/// # Errors
///
/// As [`remote_url`].
pub fn resolved_remote_url(
    repo: &Path,
    remote: &str,
    home: Option<&Path>,
) -> Result<Option<String>, RepoError> {
    let Some(url) = remote_url(repo, remote)? else {
        return Ok(None);
    };
    let config = home
        .and_then(|home| std::fs::read_to_string(home.join(SSH_DIR).join(SSH_CONFIG)).ok())
        .unwrap_or_default();
    Ok(Some(resolve_ssh_alias(&url, &config).into_owned()))
}

/// # Errors
///
/// Returns [`RepoError::Git`] when either commit is not in the local
/// repository, as before fetching a branch pushed from elsewhere.
pub fn diff_between(
    repo: &Path,
    base: &str,
    head: &str,
    paths: &[&str],
) -> Result<String, RepoError> {
    let mut args: Vec<&str> = DIFF_ARGS.to_vec();
    args.extend([base, head, PATHSPEC_SEPARATOR]);
    args.extend(paths);
    let stdout = run_git(repo, &args)?;
    Ok(hunks_only(&String::from_utf8_lossy(&stdout)))
}

fn hunks_only(diff: &str) -> String {
    diff.find(HUNK_PREFIX)
        .map_or_else(String::new, |start| diff[start..].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn git(repo: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(args)
            .output()?
            .status;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git {args:?} failed").into())
        }
    }

    fn repo_with_feature_branch(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let repo = std::env::temp_dir().join(format!(
            "krad-repo-branch-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(repo.join("src"))?;
        git(&repo, &["init", "-q", "-b", "main"])?;
        fs::write(repo.join("src").join("lib.rs"), "main\n")?;
        fs::write(repo.join("README.md"), "readme\n")?;
        git(&repo, &["add", "."])?;
        git(&repo, &["commit", "-q", "-m", "main"])?;
        git(&repo, &["checkout", "-q", "-b", "feature"])?;
        fs::create_dir_all(repo.join("docs"))?;
        fs::write(repo.join("docs").join("notes.md"), "notes\n")?;
        fs::write(repo.join("src").join("lib.rs"), "feature\n")?;
        git(&repo, &["add", "."])?;
        git(&repo, &["commit", "-q", "-m", "feature"])?;
        git(&repo, &["checkout", "-q", "main"])?;
        Ok(repo)
    }

    #[test]
    fn a_local_diff_holds_only_the_hunks_of_the_asked_paths() -> Result<(), Box<dyn Error>> {
        let repo = repo_with_feature_branch("diff")?;

        let diff = diff_between(&repo, "main", "feature", &["src/lib.rs"]);
        let missing = diff_between(&repo, "main", "0123456789abcdef", &["src/lib.rs"]);
        fs::remove_dir_all(&repo)?;

        assert_eq!(diff?, "@@ -1 +1 @@\n-main\n+feature\n");
        assert!(matches!(missing, Err(RepoError::Git { .. })));
        Ok(())
    }

    #[test]
    fn the_origin_url_is_read_and_a_missing_remote_is_none() -> Result<(), Box<dyn Error>> {
        let repo = repo_with_feature_branch("remote")?;
        let missing = remote_url(&repo, ORIGIN_REMOTE);
        git(
            &repo,
            &[
                "remote",
                "add",
                ORIGIN_REMOTE,
                "git@gitlab.example.test:team/app.git",
            ],
        )?;
        let origin = remote_url(&repo, ORIGIN_REMOTE);
        fs::remove_dir_all(&repo)?;

        assert_eq!(missing?, None);
        assert_eq!(
            origin?.as_deref(),
            Some("git@gitlab.example.test:team/app.git")
        );
        Ok(())
    }
}
