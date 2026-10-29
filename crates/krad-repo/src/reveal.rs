use std::path::Path;
use std::process::Command;

use crate::RepoError;

#[cfg(target_os = "macos")]
fn reveal_command(path: &Path) -> Command {
    const OPEN_PROGRAM: &str = "open";
    const REVEAL_FLAG: &str = "-R";
    let mut command = Command::new(OPEN_PROGRAM);
    command.arg(REVEAL_FLAG).arg(path);
    command
}

#[cfg(windows)]
fn reveal_command(path: &Path) -> Command {
    use std::ffi::OsString;

    const EXPLORER_PROGRAM: &str = "explorer.exe";
    const SELECT_FLAG: &str = "/select,";
    let mut select = OsString::from(SELECT_FLAG);
    select.push(path);
    let mut command = Command::new(EXPLORER_PROGRAM);
    command.arg(select);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_command(path: &Path) -> Command {
    const OPEN_PROGRAM: &str = "xdg-open";
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    let mut command = Command::new(OPEN_PROGRAM);
    command.arg(folder);
    command
}

const STATUS_IS_MEANINGFUL: bool = !cfg!(windows);

/// # Errors
/// Returns [`RepoError::Io`] when the launcher cannot run and [`RepoError::Reveal`] when it
/// reports a failure.
pub fn reveal_in_file_manager(path: &Path) -> Result<(), RepoError> {
    let status = reveal_command(path).status()?;
    if STATUS_IS_MEANINGFUL && !status.success() {
        return Err(RepoError::Reveal(status.to_string()));
    }
    Ok(())
}
