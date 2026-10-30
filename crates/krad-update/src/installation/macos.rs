use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::{unix, AfterInstall, Installation};
use crate::error::UpdateError;
use crate::manifest::ReleaseTarget;

const BUNDLE_LEVELS_ABOVE_EXE: usize = 3;
const BUNDLE_EXTENSION: &str = "app";
const STAGING_DIR: &str = ".krad-ade-update";
const RETIRED_BUNDLE: &str = "retired.app";
const OPEN: &str = "/usr/bin/open";
const RESOURCES_DIR: [&str; 2] = ["Contents", "Resources"];

pub(super) fn locate(exe: &Path) -> Option<Installation> {
    let bundle = exe.ancestors().nth(BUNDLE_LEVELS_ABOVE_EXE)?;
    if bundle.extension() != Some(OsStr::new(BUNDLE_EXTENSION)) {
        return None;
    }
    Some(Installation {
        root: bundle.to_path_buf(),
        staging: bundle.parent()?.join(STAGING_DIR),
        target: ReleaseTarget::MacosAarch64,
    })
}

pub(super) fn marker_dir(installation: &Installation) -> PathBuf {
    RESOURCES_DIR
        .iter()
        .fold(installation.root.clone(), |dir, part| dir.join(part))
}

pub(super) fn writable_dirs(installation: &Installation) -> Vec<&Path> {
    installation
        .root
        .parent()
        .into_iter()
        .chain([installation.root.as_path()])
        .collect()
}

pub(super) fn unpack(archive: &Path, staging: &Path) -> Result<PathBuf, UpdateError> {
    unix::untar(archive, staging)
}

pub(super) fn install(
    installation: &Installation,
    payload: &Path,
    after: AfterInstall,
) -> Result<(), UpdateError> {
    let retired = installation.staging.join(RETIRED_BUNDLE);
    fs::rename(&installation.root, &retired)?;
    if let Err(error) = fs::rename(payload, &installation.root) {
        fs::rename(&retired, &installation.root)?;
        return Err(error.into());
    }
    match after {
        AfterInstall::Relaunch => {
            unix::launch_after_exit([OsStr::new(OPEN), installation.root.as_os_str()])
        }
        AfterInstall::Exit => Ok(()),
    }
}
