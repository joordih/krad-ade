use std::ffi::OsStr;
use std::fs;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::UpdateError;

const TAR: &str = "tar";
const SHELL: &str = "/bin/sh";
const SHELL_NAME: &str = "krad-ade-relaunch";
const UNPACKED_DIR: &str = "unpacked";
const OWN_PROCESS_GROUP: i32 = 0;
const WAIT_THEN_LAUNCH: &str =
    r#"pid=$1; shift; while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done; exec "$@""#;

pub(super) fn untar(archive: &Path, staging: &Path) -> Result<PathBuf, UpdateError> {
    let into = staging.join(UNPACKED_DIR);
    fs::create_dir_all(&into)?;
    let status = Command::new(TAR)
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&into)
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(UpdateError::Tool(TAR));
    }
    for entry in fs::read_dir(&into)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            return Ok(entry.path());
        }
    }
    Err(UpdateError::MissingPayload(UNPACKED_DIR.to_owned()))
}

pub(super) fn launch_after_exit<I, S>(launch: I) -> Result<(), UpdateError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(SHELL)
        .arg("-c")
        .arg(WAIT_THEN_LAUNCH)
        .arg(SHELL_NAME)
        .arg(std::process::id().to_string())
        .args(launch)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(OWN_PROCESS_GROUP)
        .spawn()?;
    Ok(())
}
