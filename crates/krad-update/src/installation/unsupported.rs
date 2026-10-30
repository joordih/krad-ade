use std::path::{Path, PathBuf};

use super::{AfterInstall, Installation};
use crate::error::UpdateError;

pub(super) fn locate(_exe: &Path) -> Option<Installation> {
    None
}

pub(super) fn marker_dir(installation: &Installation) -> PathBuf {
    installation.root.clone()
}

pub(super) fn writable_dirs(_installation: &Installation) -> Vec<&Path> {
    Vec::new()
}

pub(super) fn unpack(_archive: &Path, _staging: &Path) -> Result<PathBuf, UpdateError> {
    Err(UpdateError::NoBuild)
}

pub(super) fn install(
    _installation: &Installation,
    _payload: &Path,
    _after: AfterInstall,
) -> Result<(), UpdateError> {
    Err(UpdateError::NoBuild)
}
