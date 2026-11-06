use krad_domain::{ExtensionEntry, ExtensionSource, GitCommit};
use std::fs;
use std::path::Path;

use super::entry::{Compatibility, IndexEntry};
use super::ExtensionIndex;
use crate::error::ExtensionError;
use crate::extension::{clone_dir, git, place_staged, remove_clone_folder};
use crate::program::read_installed;

const REMOTE: &str = "origin";
const FETCHED: &str = "FETCH_HEAD";

fn check_out(dir: &Path, commit: &GitCommit) -> Result<(), ExtensionError> {
    git(
        &["fetch", "--quiet", "--depth", "1", REMOTE, commit.as_ref()],
        Some(dir),
    )?;
    git(&["checkout", "--quiet", "--detach", FETCHED], Some(dir))
}

fn refuse_incompatible(entry: &IndexEntry) -> Result<(), ExtensionError> {
    match entry.compatibility() {
        Compatibility::Supported => Ok(()),
        Compatibility::NeedsNewerApp => Err(ExtensionError::Incompatible {
            extension: entry.name.clone(),
            wanted: entry.api,
        }),
    }
}

/// # Errors
///
/// Returns [`ExtensionError`] when the entry needs a newer app, or git cannot fetch its pinned
/// commit.
#[tracing::instrument(skip_all, fields(extension = %entry.id))]
pub fn add_from_index(
    config_dir: &Path,
    entry: &IndexEntry,
) -> Result<ExtensionEntry, ExtensionError> {
    refuse_incompatible(entry)?;
    let staging = clone_dir(config_dir, &entry.id).with_file_name(format!(
        ".incoming-index-{}-{}",
        std::process::id(),
        entry.id
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let installed = git(&["init", "--quiet"], Some(&staging))
        .and_then(|()| {
            git(
                &["remote", "add", REMOTE, entry.repository.as_ref()],
                Some(&staging),
            )
        })
        .and_then(|()| check_out(&staging, &entry.commit))
        .and_then(|()| refuse_other_id(&staging, entry))
        .and_then(|()| place_staged(config_dir, &staging, &entry.id));
    if let Err(error) = installed {
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        return Err(error);
    }
    Ok(ExtensionEntry {
        id: entry.id.clone(),
        source: ExtensionSource::Pinned {
            url: entry.repository.to_string(),
            commit: entry.commit.clone(),
        },
        enabled: true,
        trusted: None,
    })
}

fn refuse_other_id(folder: &Path, entry: &IndexEntry) -> Result<(), ExtensionError> {
    match read_installed(folder)? {
        Some(installed) if installed.manifest.id != entry.id => Err(ExtensionError::IdMismatch {
            entry: entry.id.to_string(),
            manifest: installed.manifest.id.to_string(),
        }),
        _ => Ok(()),
    }
}

/// # Errors
///
/// Returns [`ExtensionError`] when the newer commit needs a newer app or git cannot fetch it.
#[tracing::instrument(skip_all, fields(extension = %installed.id))]
pub fn update_from_index(
    config_dir: &Path,
    installed: &ExtensionEntry,
    index: &ExtensionIndex,
) -> Result<Option<ExtensionEntry>, ExtensionError> {
    let ExtensionSource::Pinned { url, commit } = &installed.source else {
        return Ok(None);
    };
    let Some(entry) = index.get(installed.id.as_ref()) else {
        return Ok(None);
    };
    if entry.commit == *commit {
        return Ok(None);
    }
    refuse_incompatible(entry)?;
    let dir = clone_dir(config_dir, &installed.id);
    check_out(&dir, &entry.commit)?;
    if let Err(error) = refuse_other_id(&dir, entry) {
        remove_clone_folder(config_dir, &dir)?;
        return Err(error);
    }
    Ok(Some(ExtensionEntry {
        source: ExtensionSource::Pinned {
            url: url.clone(),
            commit: entry.commit.clone(),
        },
        trusted: None,
        ..installed.clone()
    }))
}
