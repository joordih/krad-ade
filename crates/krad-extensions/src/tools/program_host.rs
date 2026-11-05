use krad_domain::{ServerLaunch, ServerRuntimeId, ServerRuntimePaths};
use krad_extension_api::manifest::{BinarySource, Launch};
use krad_host::{HostError, HostLink, HostPlatform};
use std::path::{Path, PathBuf};

use super::program_launch::inside;
use super::LanguageTools;
use crate::error::ExtensionError;
use crate::platform::Platform;
use crate::program::InstalledExtension;
use crate::runtime_requirements::RuntimeRequirements;
use crate::server_runtime::FindServerRuntime as _;

const EXTENSIONS_DIR: &str = "extensions";
const UPLOADED_MARKER: &str = ".krad-uploaded";
const HOST_PROGRAMS_DIR: &str = "programs";
const FINGERPRINT_CHARS: usize = 16;
const UNNAMED_FOLDER: &str = "extension";

#[derive(Debug, Clone, PartialEq)]
pub struct ProgramSite {
    pub launch: ServerLaunch,
    pub folder: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostCopy {
    home: PathBuf,
    folder: PathBuf,
    marker: PathBuf,
}

fn host_copy(extensions: &Path, installed: &InstalledExtension) -> HostCopy {
    let fingerprint = installed.fingerprint().0;
    let short: String = fingerprint.chars().take(FINGERPRINT_CHARS).collect();
    let home = extensions.join(installed.manifest.id.as_ref()).join(short);
    let name = installed
        .folder
        .file_name()
        .map_or_else(|| UNNAMED_FOLDER.into(), ToOwned::to_owned);
    HostCopy {
        folder: home.join(name),
        marker: home.join(UPLOADED_MARKER),
        home,
    }
}

fn not_connected() -> ExtensionError {
    ExtensionError::Host(HostError::NotConnected)
}

fn upload_extension(
    link: &HostLink,
    extensions: &Path,
    installed: &InstalledExtension,
) -> Result<PathBuf, ExtensionError> {
    let copy = host_copy(extensions, installed);
    if link.exists(&copy.marker) {
        return Ok(copy.folder);
    }
    if let Some(versions) = copy.home.parent() {
        if let Ok(entries) = link.read_dir(versions) {
            for stale in entries.iter().filter(|entry| entry.path != copy.home) {
                link.remove_dir_all(&stale.path)?;
            }
        }
    }
    if link.exists(&copy.home) {
        link.remove_dir_all(&copy.home)?;
    }
    link.create_dir_all(&copy.home)?;
    let folder = link.upload(&installed.folder, &copy.home)?;
    link.write(&copy.marker, &[])?;
    tracing::info!(extension = %installed.manifest.id, "extension uploaded to its host");
    Ok(folder)
}

impl LanguageTools {
    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the host is not connected, the extension cannot be uploaded
    /// there, its server runtime is missing on the host, it has no build for the host's platform or
    /// a bundled path leaves its folder.
    #[tracing::instrument(skip_all, fields(extension = %installed.manifest.id, local = link.is_local()))]
    pub fn program_site(
        &self,
        link: &HostLink,
        platform: Option<HostPlatform>,
        installed: &InstalledExtension,
        chosen: &ServerRuntimePaths,
    ) -> Result<ProgramSite, ExtensionError> {
        let HostLink::Remote(remote) = link else {
            return Ok(ProgramSite {
                launch: self.program_launch(installed, chosen)?,
                folder: installed.folder.clone(),
            });
        };
        let layout = remote.layout().ok_or_else(not_connected)?;
        let platform: Platform = platform.ok_or_else(not_connected)?.into();
        let program = installed
            .manifest
            .program
            .as_ref()
            .ok_or_else(|| ExtensionError::Missing(installed.manifest.name.clone()))?;
        let extensions = layout.app_dir().join(EXTENSIONS_DIR);
        let folder = upload_extension(link, &extensions, installed)?;
        let launch = match &program.launch {
            Launch::Runtime { runtime, args } => {
                let id = ServerRuntimeId::new(runtime.as_ref());
                let unconstrained = RuntimeRequirements::new();
                let finder = self.host_server_runtimes(link, &unconstrained);
                let found = finder
                    .find(&id)
                    .ok_or_else(|| ExtensionError::MissingServerRuntime(id.clone()))?;
                let env = finder.launch_env(&[(id, found.clone())]);
                ServerLaunch {
                    program: found,
                    args: args.clone(),
                    env,
                }
            }
            Launch::Binary { binary, args } => {
                let build = binary
                    .iter()
                    .find(|build| platform.matches(&build.targets))
                    .ok_or(ExtensionError::NoBuild)?;
                let program = match &build.source {
                    BinarySource::Bundled { path } => inside(&folder, path)?,
                    BinarySource::Download { url, sha256 } => {
                        let local =
                            self.downloaded_binary(installed, url, sha256, platform.exe())?;
                        let into = folder.join(HOST_PROGRAMS_DIR).join(sha256);
                        link.create_dir_all(&into)?;
                        link.upload(&local, &into)?
                    }
                };
                ServerLaunch {
                    program,
                    args: args.clone(),
                    env: Vec::new(),
                }
            }
        };
        Ok(ProgramSite { launch, folder })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::read_installed;
    use krad_extension_api::manifest::MANIFEST_FILE;
    use std::fs;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const MANIFEST: &str = r#"
id = "docker"
name = "Docker"
version = "0.1.0"
api = "1.0"

[program]
placement = "environment"
runtime = "node"
args = ["main.js"]

[[contributes.command]]
id = "refresh"
title = "Refresh"
"#;

    fn scratch(name: &str) -> std::io::Result<PathBuf> {
        let folder =
            std::env::temp_dir().join(format!("krad-program-host-{name}-{}", std::process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }
        fs::create_dir_all(&folder)?;
        Ok(folder)
    }

    fn installed_in(
        root: &Path,
        manifest: &str,
    ) -> Result<InstalledExtension, Box<dyn std::error::Error>> {
        let folder = root.join("krad-docker");
        fs::create_dir_all(&folder)?;
        fs::write(folder.join(MANIFEST_FILE), manifest)?;
        fs::write(folder.join("main.js"), "first")?;
        Ok(read_installed(&folder)?.ok_or("the folder holds an extension")?)
    }

    #[test]
    fn a_host_copy_lives_under_the_extension_and_its_fingerprint() -> TestResult {
        let root = scratch("layout")?;
        let installed = installed_in(&root, MANIFEST)?;
        let copy = host_copy(Path::new("/home/dev/.krad-ade/extensions"), &installed);
        let short: String = installed
            .fingerprint()
            .0
            .chars()
            .take(FINGERPRINT_CHARS)
            .collect();
        let home = PathBuf::from("/home/dev/.krad-ade/extensions/docker").join(short);
        assert_eq!(copy.folder, home.join("krad-docker"));
        assert_eq!(copy.marker, home.join(UPLOADED_MARKER));
        assert_eq!(copy.home, home);
        Ok(())
    }

    #[test]
    fn an_uploaded_copy_is_reused_until_the_extension_changes() -> TestResult {
        let root = scratch("upload")?;
        let host = root.join("host");
        let installed = installed_in(&root, MANIFEST)?;
        let first = upload_extension(&HostLink::Local, &host, &installed)?;
        assert_eq!(fs::read_to_string(first.join("main.js"))?, "first");
        fs::write(installed.folder.join("main.js"), "second")?;
        let again = upload_extension(&HostLink::Local, &host, &installed)?;
        assert_eq!(again, first);
        assert_eq!(fs::read_to_string(again.join("main.js"))?, "first");
        let changed = installed_in(&root, &MANIFEST.replace("Refresh", "Refresh all"))?;
        let updated = upload_extension(&HostLink::Local, &host, &changed)?;
        assert_ne!(updated, first);
        assert_eq!(fs::read_to_string(updated.join("main.js"))?, "first");
        assert!(!first.exists(), "the copy of the older version is removed");
        Ok(())
    }
}
