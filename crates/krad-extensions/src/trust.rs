use krad_domain::{ExtensionEntry, ExtensionFingerprint, ExtensionSource, GitCommit};
use krad_extension_api::manifest::ExtensionManifest;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::content::{content_digest, ContentDigest};
use crate::error::ExtensionError;
use crate::extension::{folder_of, user_manifests_dir};
use crate::manifest::{summary_of, ManifestSummary};
use crate::program::{read_installed, InstalledExtension};

const GIT: &str = "git";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    Trusted,
    Asked,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Source<'a> {
    Folder,
    Git { head: String },
    Pinned { commit: &'a GitCommit },
}

#[derive(Serialize)]
struct Fingerprinted<'a> {
    manifest: Option<&'a ExtensionManifest>,
    summary: &'a ManifestSummary,
    source: Source<'a>,
    content: &'a str,
}

#[derive(Debug, Clone)]
pub struct Inspected {
    pub entry: ExtensionEntry,
    pub folder: PathBuf,
    pub installed: Option<InstalledExtension>,
    pub summary: ManifestSummary,
    pub fingerprint: ExtensionFingerprint,
}

#[derive(Debug, Clone)]
pub struct TrustedExtension {
    folder: PathBuf,
    installed: Option<InstalledExtension>,
    fingerprint: ExtensionFingerprint,
}

#[derive(Debug, Clone)]
pub struct TrustedProgram {
    installed: InstalledExtension,
    fingerprint: ExtensionFingerprint,
}

fn git_head(dir: &Path) -> Result<String, ExtensionError> {
    let output = Command::new(GIT)
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(ExtensionError::Tool(GIT.to_owned()));
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    GitCommit::try_from(head.clone()).map_err(|_| ExtensionError::Tool(GIT.to_owned()))?;
    Ok(head)
}

/// # Errors
///
/// Returns [`ExtensionError`] when the extension's folder cannot be read, its manifest is not
/// valid or names another id than the extension, the folder is larger than an extension may be,
/// or its git commit cannot be read. Nothing in the folder is run.
pub fn inspect(config_dir: &Path, entry: ExtensionEntry) -> Result<Inspected, ExtensionError> {
    let folder = folder_of(config_dir, &entry);
    let installed = read_installed(&folder)?;
    if let Some(installed) = &installed {
        if installed.manifest.id != entry.id {
            return Err(ExtensionError::IdMismatch {
                entry: entry.id.to_string(),
                manifest: installed.manifest.id.to_string(),
            });
        }
    }
    let summary = summary_of(&folder)?;
    let ContentDigest(content) = content_digest(&folder)?;
    let source = match &entry.source {
        ExtensionSource::Folder(_) => Source::Folder,
        ExtensionSource::Git(_) => Source::Git {
            head: git_head(&folder)?,
        },
        ExtensionSource::Pinned { commit, .. } => {
            let head = git_head(&folder)?;
            if head != commit.as_ref() {
                return Err(ExtensionError::CommitMoved);
            }
            Source::Pinned { commit }
        }
    };
    let fingerprinted = Fingerprinted {
        manifest: installed.as_ref().map(|installed| &installed.manifest),
        summary: &summary,
        source,
        content: &content,
    };
    let bytes = serde_json::to_vec(&fingerprinted)?;
    let fingerprint = ExtensionFingerprint(format!("{:x}", Sha256::digest(bytes)));
    Ok(Inspected {
        entry,
        folder,
        installed,
        summary,
        fingerprint,
    })
}

impl Inspected {
    #[must_use]
    pub fn trust(&self) -> Trust {
        if self.entry.trusted.as_ref() == Some(&self.fingerprint) {
            Trust::Trusted
        } else {
            Trust::Asked
        }
    }

    #[must_use]
    pub fn trusted(&self) -> Option<TrustedExtension> {
        (self.entry.enabled && self.trust() == Trust::Trusted).then(|| TrustedExtension {
            folder: self.folder.clone(),
            installed: self.installed.clone(),
            fingerprint: self.fingerprint.clone(),
        })
    }

    #[must_use]
    pub fn manifest(&self) -> Option<&ExtensionManifest> {
        self.installed.as_ref().map(|installed| &installed.manifest)
    }
}

impl TrustedExtension {
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    #[must_use]
    pub fn program(&self) -> Option<TrustedProgram> {
        let installed = self.installed.clone()?;
        installed.manifest.program.as_ref()?;
        Some(TrustedProgram {
            installed,
            fingerprint: self.fingerprint.clone(),
        })
    }
}

impl TrustedProgram {
    #[must_use]
    pub fn manifest(&self) -> &ExtensionManifest {
        &self.installed.manifest
    }

    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.installed.folder
    }

    #[must_use]
    pub fn fingerprint(&self) -> &ExtensionFingerprint {
        &self.fingerprint
    }
}

#[must_use]
pub fn trusted_manifest_folders(config_dir: &Path, entries: &[ExtensionEntry]) -> Vec<PathBuf> {
    entries
        .iter()
        .filter(|entry| entry.enabled && entry.trusted.is_some())
        .filter_map(|entry| match inspect(config_dir, entry.clone()) {
            Ok(inspected) => inspected.trusted().map(|trusted| trusted.folder),
            Err(error) => {
                tracing::debug!(extension = %entry.id, %error, "extension not inspected");
                None
            }
        })
        .chain(std::iter::once(user_manifests_dir(config_dir)))
        .collect()
}

#[cfg(test)]
mod tests;
