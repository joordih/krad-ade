use std::fs;
use std::path::{Path, PathBuf};

use super::{unix, AfterInstall, Installation};
use crate::error::UpdateError;
use crate::manifest::ReleaseTarget;

const STAGING_DIR: &str = ".krad-ade-update";
const RETIRED_DIR: &str = "retired";
const APP_BINARY: &str = "krad-ade";

pub(super) fn locate(exe: &Path) -> Option<Installation> {
    let root = exe.parent()?;
    Some(Installation {
        root: root.to_path_buf(),
        staging: root.join(STAGING_DIR),
        target: ReleaseTarget::LinuxX86_64,
    })
}

pub(super) fn marker_dir(installation: &Installation) -> PathBuf {
    installation.root.clone()
}

pub(super) fn writable_dirs(installation: &Installation) -> Vec<&Path> {
    vec![installation.root.as_path()]
}

pub(super) fn unpack(archive: &Path, staging: &Path) -> Result<PathBuf, UpdateError> {
    unix::untar(archive, staging)
}

fn restore(moved: &[(PathBuf, Option<PathBuf>)]) -> Result<(), UpdateError> {
    for (installed, retired) in moved.iter().rev() {
        match retired {
            Some(retired) => fs::rename(retired, installed)?,
            None => fs::remove_file(installed)?,
        }
    }
    Ok(())
}

fn replace_entries(
    installation: &Installation,
    payload: &Path,
    moved: &mut Vec<(PathBuf, Option<PathBuf>)>,
) -> Result<(), UpdateError> {
    let retired_dir = installation.staging.join(RETIRED_DIR);
    fs::create_dir_all(&retired_dir)?;
    for entry in fs::read_dir(payload)? {
        let entry = entry?;
        let installed = installation.root.join(entry.file_name());
        let retired = installed
            .exists()
            .then(|| retired_dir.join(entry.file_name()));
        if let Some(retired) = &retired {
            fs::rename(&installed, retired)?;
        }
        moved.push((installed.clone(), retired));
        fs::rename(entry.path(), &installed)?;
    }
    Ok(())
}

pub(super) fn install(
    installation: &Installation,
    payload: &Path,
    after: AfterInstall,
) -> Result<(), UpdateError> {
    let mut moved = Vec::new();
    if let Err(error) = replace_entries(installation, payload, &mut moved) {
        restore(&moved)?;
        return Err(error);
    }
    match after {
        AfterInstall::Relaunch => unix::launch_after_exit([installation.root.join(APP_BINARY)]),
        AfterInstall::Exit => Ok(()),
    }
}
