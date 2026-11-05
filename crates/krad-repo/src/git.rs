use krad_host::HostLink;
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use crate::process::{git_output, git_output_within, host_path, GitEnv, GIT_PROGRAM};
use crate::RepoError;

const NOT_A_REPOSITORY: &str = "not a git repository";
const TOPLEVEL_ARGS: [&str; 2] = ["rev-parse", "--show-toplevel"];
const ARG_SEPARATOR: &str = " ";

pub(crate) fn run_git<S: AsRef<OsStr>>(
    host: &HostLink,
    repo: &Path,
    args: &[S],
) -> Result<Vec<u8>, RepoError> {
    checked(git_output(host, repo, args)?, args)
}

pub(crate) fn run_git_within<S: AsRef<OsStr>>(
    host: &HostLink,
    repo: &Path,
    args: &[S],
    env: &GitEnv,
    timeout: Duration,
) -> Result<Vec<u8>, RepoError> {
    checked(git_output_within(host, repo, args, env, timeout)?, args)
}

fn checked<S: AsRef<OsStr>>(output: Output, args: &[S]) -> Result<Vec<u8>, RepoError> {
    if output.status.success() {
        return Ok(output.stdout);
    }
    Err(RepoError::Git {
        command: command_line(args),
        message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

fn command_line<S: AsRef<OsStr>>(args: &[S]) -> String {
    std::iter::once(GIT_PROGRAM.into())
        .chain(args.iter().map(|arg| arg.as_ref().to_string_lossy()))
        .collect::<Vec<_>>()
        .join(ARG_SEPARATOR)
}

/// # Errors
pub fn repo_root(host: &HostLink, path: &Path) -> Result<Option<PathBuf>, RepoError> {
    match run_git(host, path, &TOPLEVEL_ARGS) {
        Ok(stdout) => Ok(Some(host_path(
            host,
            path,
            String::from_utf8_lossy(&stdout).trim_end(),
        ))),
        Err(RepoError::Io(error)) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(RepoError::Git { message, .. }) if message.contains(NOT_A_REPOSITORY) => Ok(None),
        Err(RepoError::Git { .. }) if !host.exists(path) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_plain_folder_has_no_repository() -> Result<(), RepoError> {
        let base = std::env::temp_dir().join(format!("krad-repo-root-{}", std::process::id()));
        std::fs::create_dir_all(&base)?;
        let host = HostLink::Local;
        assert_eq!(repo_root(&host, &base.join("docs/notes"))?, None);
        assert_eq!(repo_root(&host, &base)?, None);
        std::fs::remove_dir_all(&base)?;
        Ok(())
    }
}
