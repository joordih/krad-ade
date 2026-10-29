use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::process::{git_command, host_path, GIT_PROGRAM};
use crate::RepoError;

const NOT_A_REPOSITORY: &str = "not a git repository";
const TOPLEVEL_ARGS: [&str; 2] = ["rev-parse", "--show-toplevel"];
const ARG_SEPARATOR: &str = " ";

pub(crate) fn run_git<S: AsRef<OsStr>>(repo: &Path, args: &[S]) -> Result<Vec<u8>, RepoError> {
    let output = git_command(repo).args(args).output()?;
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
pub fn repo_root(path: &Path) -> Result<Option<PathBuf>, RepoError> {
    match run_git(path, &TOPLEVEL_ARGS) {
        Ok(stdout) => Ok(Some(host_path(
            path,
            String::from_utf8_lossy(&stdout).trim_end(),
        ))),
        Err(RepoError::Io(error)) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(RepoError::Git { message, .. }) if message.contains(NOT_A_REPOSITORY) => Ok(None),
        Err(error) => Err(error),
    }
}
