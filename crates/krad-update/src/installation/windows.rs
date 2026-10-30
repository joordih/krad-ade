use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{AfterInstall, Installation};
use crate::error::UpdateError;
use crate::manifest::ReleaseTarget;

const STAGING_DIR: &str = "krad-ade-update";
const SILENT_SETUP_FLAGS: [&str; 5] = [
    "/VERYSILENT",
    "/SUPPRESSMSGBOXES",
    "/NORESTART",
    "/CLOSEAPPLICATIONS",
    "/CURRENTUSER",
];
const DIR_FLAG: &str = "/DIR=";
const RELAUNCH_FLAG: &str = "/RELAUNCH=yes";

pub(super) fn locate(exe: &Path) -> Option<Installation> {
    Some(Installation {
        root: exe.parent()?.to_path_buf(),
        staging: std::env::temp_dir().join(STAGING_DIR),
        target: ReleaseTarget::WindowsX86_64,
    })
}

pub(super) fn marker_dir(installation: &Installation) -> PathBuf {
    installation.root.clone()
}

pub(super) fn writable_dirs(installation: &Installation) -> Vec<&Path> {
    vec![installation.root.as_path()]
}

pub(super) fn unpack(archive: &Path, _staging: &Path) -> Result<PathBuf, UpdateError> {
    Ok(fs::canonicalize(archive)?)
}

pub(super) fn install(
    installation: &Installation,
    payload: &Path,
    after: AfterInstall,
) -> Result<(), UpdateError> {
    let mut dir_flag = std::ffi::OsString::from(DIR_FLAG);
    dir_flag.push(&installation.root);
    let mut setup = Command::new(payload);
    setup.args(SILENT_SETUP_FLAGS).arg(dir_flag);
    if after == AfterInstall::Relaunch {
        setup.arg(RELAUNCH_FLAG);
    }
    setup.spawn()?;
    Ok(())
}
