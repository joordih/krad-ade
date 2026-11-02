use krad_domain::{ExtensionEntry, ExtensionSource};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::ExtensionError;

const EXTENSIONS_DIR: &str = "extensions";
const MANIFESTS_DIR: &str = "manifests";
const GIT: &str = "git";
const GIT_SUFFIX: &str = ".git";
const URL_SEPARATORS: [char; 2] = ['/', ':'];

#[must_use]
pub fn user_manifests_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(MANIFESTS_DIR)
}

fn clone_dir(config_dir: &Path, id: &str) -> PathBuf {
    config_dir.join(EXTENSIONS_DIR).join(id)
}

#[must_use]
pub fn folder_of(config_dir: &Path, entry: &ExtensionEntry) -> PathBuf {
    match &entry.source {
        ExtensionSource::Git(_) => clone_dir(config_dir, &entry.id),
        ExtensionSource::Folder(folder) => folder.clone(),
    }
}

#[must_use]
pub fn manifest_folders(config_dir: &Path, entries: &[ExtensionEntry]) -> Vec<PathBuf> {
    entries
        .iter()
        .filter(|entry| entry.enabled)
        .map(|entry| folder_of(config_dir, entry))
        .chain(std::iter::once(user_manifests_dir(config_dir)))
        .collect()
}

fn id_from(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit(URL_SEPARATORS).next()?;
    let id = last.strip_suffix(GIT_SUFFIX).unwrap_or(last);
    (!id.is_empty()).then(|| id.to_owned())
}

fn git(args: &[&str], dir: Option<&Path>) -> Result<(), ExtensionError> {
    let mut command = Command::new(GIT);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    if command.status()?.success() {
        Ok(())
    } else {
        Err(ExtensionError::Tool(GIT.to_owned()))
    }
}

/// # Errors
///
/// Returns [`ExtensionError`] when the address names no extension or git cannot clone it.
pub fn add_git(config_dir: &Path, url: &str) -> Result<ExtensionEntry, ExtensionError> {
    let id = id_from(url).ok_or_else(|| ExtensionError::Missing(url.to_owned()))?;
    let dir = clone_dir(config_dir, &id);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }
    git(
        &["clone", "--depth", "1", url, &dir.to_string_lossy()],
        None,
    )?;
    Ok(ExtensionEntry {
        id,
        source: ExtensionSource::Git(url.to_owned()),
        enabled: true,
    })
}

/// # Errors
///
/// Returns [`ExtensionError::Missing`] when the folder does not exist.
pub fn add_folder(folder: &Path) -> Result<ExtensionEntry, ExtensionError> {
    if !folder.is_dir() {
        return Err(ExtensionError::Missing(
            folder.to_string_lossy().into_owned(),
        ));
    }
    let id = folder.file_name().map_or_else(
        || folder.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    Ok(ExtensionEntry {
        id,
        source: ExtensionSource::Folder(folder.to_path_buf()),
        enabled: true,
    })
}

/// # Errors
///
/// Returns [`ExtensionError`] when git cannot pull a cloned extension.
pub fn update(config_dir: &Path, entry: &ExtensionEntry) -> Result<(), ExtensionError> {
    match entry.source {
        ExtensionSource::Git(_) => git(
            &["pull", "--ff-only"],
            Some(&clone_dir(config_dir, &entry.id)),
        ),
        ExtensionSource::Folder(_) => Ok(()),
    }
}

/// # Errors
///
/// Returns [`ExtensionError::Io`] when a cloned extension cannot be deleted.
pub fn remove(config_dir: &Path, entry: &ExtensionEntry) -> Result<(), ExtensionError> {
    let dir = clone_dir(config_dir, &entry.id);
    if matches!(entry.source, ExtensionSource::Git(_)) && dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_git_address_names_its_extension() {
        assert_eq!(
            id_from("https://github.com/acme/krad-zig.git").as_deref(),
            Some("krad-zig")
        );
        assert_eq!(
            id_from("git@github.com:acme/krad-zig").as_deref(),
            Some("krad-zig")
        );
        assert_eq!(id_from("").as_deref(), None);
    }

    #[test]
    fn disabled_extensions_are_left_out_and_the_user_folder_comes_last() {
        let config = Path::new("/config");
        let entries = [
            ExtensionEntry {
                id: "zig".to_owned(),
                source: ExtensionSource::Git("https://x/zig.git".to_owned()),
                enabled: true,
            },
            ExtensionEntry {
                id: "off".to_owned(),
                source: ExtensionSource::Folder(PathBuf::from("/elsewhere")),
                enabled: false,
            },
        ];
        let folders = manifest_folders(config, &entries);
        assert_eq!(
            folders,
            [
                config.join("extensions").join("zig"),
                config.join("manifests")
            ]
        );
    }
}
