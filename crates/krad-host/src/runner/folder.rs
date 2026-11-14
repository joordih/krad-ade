use krad_domain::InstallId;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::mode::FileMode;

const CLIENTS_DIR: &str = "clients";
const PAIRING_EXTENSION: &str = "pairing";

/// # Errors
///
/// Fails when the folder cannot be made or its mode cannot be set.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(FileMode::PRIVATE_DIR.0))
}

#[must_use]
pub fn clients_dir(runner_dir: &Path) -> PathBuf {
    runner_dir.join(CLIENTS_DIR)
}

#[must_use]
pub fn pairing_file(runner_dir: &Path, install: &InstallId) -> PathBuf {
    clients_dir(runner_dir)
        .join(install.as_str())
        .with_extension(PAIRING_EXTENSION)
}

#[must_use]
pub fn paired_installs(runner_dir: &Path) -> Vec<InstallId> {
    let Ok(entries) = fs::read_dir(clients_dir(runner_dir)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == PAIRING_EXTENSION))
        .filter_map(|path| path.file_stem()?.to_str()?.parse().ok())
        .collect()
}
