use krad_domain::{ExtensionEntry, ExtensionSource, GitCommit};
use std::fs;
use std::path::Path;

use super::entry::{Compatibility, IndexEntry};
use super::ExtensionIndex;
use crate::error::ExtensionError;
use crate::extension::{clone_dir, git};

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
    let id = entry.id.to_string();
    let dir = clone_dir(config_dir, &id);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    let installed = git(&["init", "--quiet"], Some(&dir))
        .and_then(|()| {
            git(
                &["remote", "add", REMOTE, entry.repository.as_ref()],
                Some(&dir),
            )
        })
        .and_then(|()| check_out(&dir, &entry.commit));
    if let Err(error) = installed {
        fs::remove_dir_all(&dir)?;
        return Err(error);
    }
    Ok(ExtensionEntry {
        id,
        source: ExtensionSource::Pinned {
            url: entry.repository.to_string(),
            commit: entry.commit.clone(),
        },
        enabled: true,
        trusted: None,
    })
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
    let Some(entry) = index.get(&installed.id) else {
        return Ok(None);
    };
    if entry.commit == *commit {
        return Ok(None);
    }
    refuse_incompatible(entry)?;
    check_out(&clone_dir(config_dir, &installed.id), &entry.commit)?;
    Ok(Some(ExtensionEntry {
        source: ExtensionSource::Pinned {
            url: url.clone(),
            commit: entry.commit.clone(),
        },
        ..installed.clone()
    }))
}
