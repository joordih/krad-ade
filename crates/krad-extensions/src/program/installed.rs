use krad_domain::{ExtensionEntry, ExtensionFingerprint};
use krad_extension_api::manifest::{
    Activation, Contributions, ExtensionManifest, Program, MANIFEST_FILE,
};
use krad_extension_api::{ApiVersion, CURRENT_API};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::ExtensionError;

#[derive(Debug, Clone, PartialEq)]
pub struct InstalledExtension {
    pub folder: PathBuf,
    pub manifest: ExtensionManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    NotNeeded,
    Trusted,
    Asked,
}

#[derive(Serialize)]
struct Declared<'a> {
    api: ApiVersion,
    program: Option<&'a Program>,
    activation: &'a [Activation],
    contributes: &'a Contributions,
}

impl InstalledExtension {
    #[must_use]
    pub fn fingerprint(&self) -> ExtensionFingerprint {
        let declared = Declared {
            api: self.manifest.api,
            program: self.manifest.program.as_ref(),
            activation: &self.manifest.activation,
            contributes: &self.manifest.contributes,
        };
        let bytes = serde_json::to_vec(&declared).unwrap_or_default();
        ExtensionFingerprint(format!("{:x}", Sha256::digest(bytes)))
    }

    #[must_use]
    pub fn trust(&self, entry: &ExtensionEntry) -> Trust {
        if self.manifest.program.is_none() {
            return Trust::NotNeeded;
        }
        if entry.trusted.as_ref() == Some(&self.fingerprint()) {
            Trust::Trusted
        } else {
            Trust::Asked
        }
    }
}

/// # Errors
///
/// Returns [`ExtensionError`] when the folder's `extension.toml` cannot be read, is not a valid
/// manifest, contradicts itself or needs a newer API than the app offers.
pub fn read_installed(folder: &Path) -> Result<Option<InstalledExtension>, ExtensionError> {
    let file = folder.join(MANIFEST_FILE);
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let invalid = |reason: String| ExtensionError::Manifest {
        file: file.to_string_lossy().into_owned(),
        reason,
    };
    let manifest: ExtensionManifest =
        toml::from_str(&text).map_err(|error| invalid(error.to_string()))?;
    manifest
        .check()
        .map_err(|error| invalid(error.to_string()))?;
    if !CURRENT_API.supports(manifest.api) {
        return Err(ExtensionError::Incompatible {
            extension: manifest.name,
            wanted: manifest.api,
        });
    }
    Ok(Some(InstalledExtension {
        folder: folder.to_path_buf(),
        manifest,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ExtensionSource;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const MANIFEST: &str = r#"
id = "docker"
name = "Docker"
version = "0.1.0"
api = "1.0"

[program]
placement = "app"
runtime = "node"
args = ["main.js"]

[[contributes.command]]
id = "refresh"
title = "Docker: Refresh"
"#;

    fn folder_with(name: &str, manifest: Option<&str>) -> io::Result<PathBuf> {
        let folder =
            std::env::temp_dir().join(format!("krad-installed-{name}-{}", std::process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }
        fs::create_dir_all(&folder)?;
        if let Some(manifest) = manifest {
            fs::write(folder.join(MANIFEST_FILE), manifest)?;
        }
        Ok(folder)
    }

    fn entry(trusted: Option<ExtensionFingerprint>) -> ExtensionEntry {
        ExtensionEntry {
            id: "docker".to_owned(),
            source: ExtensionSource::Folder(PathBuf::from("/x")),
            enabled: true,
            trusted,
        }
    }

    #[test]
    fn a_folder_of_manifests_only_is_not_an_extension_program() -> TestResult {
        let folder = folder_with("plain", None)?;
        assert_eq!(read_installed(&folder)?, None);
        Ok(())
    }

    #[test]
    fn a_program_asks_for_trust_until_its_declarations_are_trusted() -> TestResult {
        let folder = folder_with("trust", Some(MANIFEST))?;
        let installed = read_installed(&folder)?.ok_or("the folder holds an extension")?;
        assert_eq!(installed.trust(&entry(None)), Trust::Asked);
        let trusted = entry(Some(installed.fingerprint()));
        assert_eq!(installed.trust(&trusted), Trust::Trusted);
        let changed = MANIFEST.replace("Docker: Refresh", "Docker: Refresh all");
        fs::write(folder.join(MANIFEST_FILE), changed)?;
        let updated = read_installed(&folder)?.ok_or("the folder holds an extension")?;
        assert_eq!(updated.trust(&trusted), Trust::Asked);
        Ok(())
    }

    #[test]
    fn a_newer_api_or_a_broken_manifest_is_refused() -> TestResult {
        let newer = folder_with(
            "newer",
            Some(&MANIFEST.replace("api = \"1.0\"", "api = \"1.9\"")),
        )?;
        assert!(matches!(
            read_installed(&newer),
            Err(ExtensionError::Incompatible { .. })
        ));
        let broken = folder_with("broken", Some(&MANIFEST.replace("[program]", "")))?;
        assert!(matches!(
            read_installed(&broken),
            Err(ExtensionError::Manifest { .. })
        ));
        Ok(())
    }
}
