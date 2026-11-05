use krad_domain::{ExtensionEntry, ExtensionId, ExtensionSource};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::ExtensionError;
use crate::program::read_installed;

const EXTENSIONS_DIR: &str = "extensions";
const MANIFESTS_DIR: &str = "manifests";
const STAGING_PREFIX: &str = ".incoming-";
const GIT: &str = "git";
const GIT_SUFFIX: &str = ".git";
const URL_SEPARATORS: [char; 2] = ['/', ':'];
const OPTION_MARK: char = '-';
const END_OF_OPTIONS: &str = "--";

static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);

#[must_use]
pub fn user_manifests_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(MANIFESTS_DIR)
}

fn extensions_root(config_dir: &Path) -> PathBuf {
    config_dir.join(EXTENSIONS_DIR)
}

pub(crate) fn clone_dir(config_dir: &Path, id: &ExtensionId) -> PathBuf {
    id.folder_in(&extensions_root(config_dir))
}

fn staging_dir(config_dir: &Path) -> PathBuf {
    let serial = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
    extensions_root(config_dir).join(format!("{STAGING_PREFIX}{}-{serial}", std::process::id()))
}

pub(crate) fn remove_clone_folder(config_dir: &Path, dir: &Path) -> Result<(), ExtensionError> {
    let root = extensions_root(config_dir);
    if dir.parent() != Some(root.as_path()) || dir == root {
        return Err(ExtensionError::OutsideFolder(dir.to_path_buf()));
    }
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[must_use]
pub fn folder_of(config_dir: &Path, entry: &ExtensionEntry) -> PathBuf {
    match &entry.source {
        ExtensionSource::Git(_) | ExtensionSource::Pinned { .. } => {
            clone_dir(config_dir, &entry.id)
        }
        ExtensionSource::Folder(folder) => folder.clone(),
    }
}

fn name_from(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit(URL_SEPARATORS).next()?;
    let name = last.strip_suffix(GIT_SUFFIX).unwrap_or(last);
    (!name.is_empty()).then(|| name.to_lowercase())
}

pub(crate) fn id_for(folder: &Path, fallback: &str) -> Result<ExtensionId, ExtensionError> {
    if let Some(installed) = read_installed(folder)? {
        return Ok(installed.manifest.id);
    }
    fallback
        .to_lowercase()
        .parse()
        .map_err(|_| ExtensionError::InvalidId(fallback.to_owned()))
}

pub(crate) fn checked_address(url: &str) -> Result<&str, ExtensionError> {
    let url = url.trim();
    if url.is_empty() || url.starts_with(OPTION_MARK) || url.chars().any(char::is_whitespace) {
        return Err(ExtensionError::BadAddress(url.to_owned()));
    }
    Ok(url)
}

pub(crate) fn git(args: &[&str], dir: Option<&Path>) -> Result<(), ExtensionError> {
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

pub(crate) fn place_staged(
    config_dir: &Path,
    staging: &Path,
    id: &ExtensionId,
) -> Result<PathBuf, ExtensionError> {
    let target = clone_dir(config_dir, id);
    remove_clone_folder(config_dir, &target)?;
    fs::rename(staging, &target)?;
    Ok(target)
}

/// # Errors
///
/// Returns [`ExtensionError`] when the address is not one git may be given, git cannot clone
/// it, or the clone names no valid extension id.
pub fn add_git(config_dir: &Path, url: &str) -> Result<ExtensionEntry, ExtensionError> {
    let url = checked_address(url)?;
    let fallback = name_from(url).ok_or_else(|| ExtensionError::Missing(url.to_owned()))?;
    fs::create_dir_all(extensions_root(config_dir))?;
    let staging = staging_dir(config_dir);
    let cloned = git(
        &[
            "clone",
            "--depth",
            "1",
            END_OF_OPTIONS,
            url,
            &staging.to_string_lossy(),
        ],
        None,
    )
    .and_then(|()| id_for(&staging, &fallback))
    .and_then(|id| place_staged(config_dir, &staging, &id).map(|_| id));
    let id = match cloned {
        Ok(id) => id,
        Err(error) => {
            if staging.exists() {
                fs::remove_dir_all(&staging)?;
            }
            return Err(error);
        }
    };
    Ok(ExtensionEntry {
        id,
        source: ExtensionSource::Git(url.to_owned()),
        enabled: true,
        trusted: None,
    })
}

/// # Errors
///
/// Returns [`ExtensionError::Missing`] when the folder does not exist, or [`ExtensionError`]
/// when it names no valid extension id.
pub fn add_folder(folder: &Path) -> Result<ExtensionEntry, ExtensionError> {
    if !folder.is_dir() {
        return Err(ExtensionError::Missing(
            folder.to_string_lossy().into_owned(),
        ));
    }
    let fallback = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(ExtensionEntry {
        id: id_for(folder, &fallback)?,
        source: ExtensionSource::Folder(folder.to_path_buf()),
        enabled: true,
        trusted: None,
    })
}

/// # Errors
///
/// Returns [`ExtensionError`] when git cannot pull a cloned extension, or
/// [`ExtensionError::Pinned`] for an extension the index pins, which moves through the index.
pub fn update(config_dir: &Path, entry: &ExtensionEntry) -> Result<ExtensionEntry, ExtensionError> {
    match entry.source {
        ExtensionSource::Git(_) => {
            git(
                &["pull", "--ff-only"],
                Some(&clone_dir(config_dir, &entry.id)),
            )?;
            Ok(ExtensionEntry {
                trusted: None,
                ..entry.clone()
            })
        }
        ExtensionSource::Pinned { .. } => Err(ExtensionError::Pinned(entry.id.to_string())),
        ExtensionSource::Folder(_) => Ok(entry.clone()),
    }
}

/// # Errors
///
/// Returns [`ExtensionError::Io`] when a cloned extension cannot be deleted.
pub fn remove(config_dir: &Path, entry: &ExtensionEntry) -> Result<(), ExtensionError> {
    match entry.source {
        ExtensionSource::Git(_) | ExtensionSource::Pinned { .. } => {
            remove_clone_folder(config_dir, &clone_dir(config_dir, &entry.id))
        }
        ExtensionSource::Folder(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_git_address_names_its_extension() {
        assert_eq!(
            name_from("https://github.com/acme/Krad-Zig.git").as_deref(),
            Some("krad-zig")
        );
        assert_eq!(
            name_from("git@github.com:acme/krad-zig").as_deref(),
            Some("krad-zig")
        );
        assert_eq!(name_from("").as_deref(), None);
    }

    #[test]
    fn addresses_that_git_would_read_as_options_are_refused() {
        for bad in ["--upload-pack=touch /tmp/x", "-c", "a b", ""] {
            assert!(
                matches!(checked_address(bad), Err(ExtensionError::BadAddress(_))),
                "{bad}"
            );
        }
        assert!(checked_address("https://github.com/acme/docker.git").is_ok());
    }

    #[test]
    fn a_clone_folder_never_leaves_the_extensions_folder() -> TestResult {
        let config = Path::new("/config");
        let id: ExtensionId = "docker".parse()?;
        assert_eq!(
            clone_dir(config, &id),
            PathBuf::from("/config/extensions/docker")
        );
        for outside in [
            "/config",
            "/config/extensions",
            "/config/extensions/a/b",
            "/etc",
        ] {
            assert!(
                matches!(
                    remove_clone_folder(config, Path::new(outside)),
                    Err(ExtensionError::OutsideFolder(_))
                ),
                "{outside}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_folder_without_a_manifest_takes_a_safe_id_from_its_name() -> TestResult {
        let folder = std::env::temp_dir().join(format!("Krad-Zig-{}", std::process::id()));
        fs::create_dir_all(&folder)?;
        let entry = add_folder(&folder)?;
        assert!(entry.id.as_ref().starts_with("krad-zig-"));
        let dots = std::env::temp_dir().join(format!("..x{}", std::process::id()));
        fs::create_dir_all(&dots)?;
        assert!(matches!(
            add_folder(&dots),
            Err(ExtensionError::InvalidId(_))
        ));
        Ok(())
    }

    #[test]
    fn updating_a_git_extension_asks_for_trust_again() -> TestResult {
        let base = std::env::temp_dir().join(format!("krad-update-{}", std::process::id()));
        if base.exists() {
            fs::remove_dir_all(&base)?;
        }
        let origin = base.join("origin");
        fs::create_dir_all(&origin)?;
        git(&["init", "--quiet"], Some(&origin))?;
        git(
            &[
                "-c",
                "user.name=krad",
                "-c",
                "user.email=krad@example.com",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "first",
            ],
            Some(&origin),
        )?;
        let config = base.join("config");
        let id: ExtensionId = "zig".parse()?;
        let dir = clone_dir(&config, &id);
        fs::create_dir_all(extensions_root(&config))?;
        git(
            &[
                "clone",
                "--quiet",
                &origin.to_string_lossy(),
                &dir.to_string_lossy(),
            ],
            None,
        )?;
        let entry = ExtensionEntry {
            id,
            source: ExtensionSource::Git(origin.to_string_lossy().into_owned()),
            enabled: true,
            trusted: Some(krad_domain::ExtensionFingerprint("old".to_owned())),
        };
        assert_eq!(update(&config, &entry)?.trusted, None);
        Ok(())
    }
}
