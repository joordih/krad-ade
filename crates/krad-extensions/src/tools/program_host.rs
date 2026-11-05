use krad_domain::{ServerLaunch, ServerRuntimeId, ServerRuntimePaths};
use krad_extension_api::manifest::{BinarySource, Launch};
use krad_host::{HostError, HostLink, HostPlatform};
use std::path::{Path, PathBuf};

use super::program_launch::contained;
use super::LanguageTools;
use crate::error::ExtensionError;
use crate::platform::Platform;
use crate::runtime_requirements::RuntimeRequirements;
use crate::server_runtime::FindServerRuntime as _;
use crate::trust::TrustedProgram;

const EXTENSIONS_DIR: &str = "extensions";
const UPLOADED_MARKER: &str = ".krad-uploaded";
const HOST_PROGRAMS_DIR: &str = "programs";
const UNNAMED_FOLDER: &str = "extension";

#[derive(Debug, Clone, PartialEq)]
pub struct ProgramSite {
    pub launch: ServerLaunch,
    pub folder: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostCopy {
    versions: PathBuf,
    home: PathBuf,
    folder: PathBuf,
    marker: PathBuf,
}

fn host_copy(extensions: &Path, program: &TrustedProgram) -> HostCopy {
    let versions = program.manifest().id.folder_in(extensions);
    let home = versions.join(&program.fingerprint().0);
    let name = program
        .folder()
        .file_name()
        .map_or_else(|| UNNAMED_FOLDER.into(), ToOwned::to_owned);
    HostCopy {
        folder: home.join(name),
        marker: home.join(UPLOADED_MARKER),
        versions,
        home,
    }
}

fn not_connected() -> ExtensionError {
    ExtensionError::Host(HostError::NotConnected)
}

fn upload_extension(
    link: &HostLink,
    extensions: &Path,
    program: &TrustedProgram,
) -> Result<PathBuf, ExtensionError> {
    let copy = host_copy(extensions, program);
    if link.exists(&copy.marker) {
        return Ok(copy.folder);
    }
    if copy.versions.parent() != Some(extensions) {
        return Err(ExtensionError::OutsideFolder(copy.versions));
    }
    if let Ok(entries) = link.read_dir(&copy.versions) {
        for stale in entries.iter().filter(|entry| entry.path != copy.home) {
            if stale.path.parent() == Some(copy.versions.as_path()) {
                link.remove_dir_all(&stale.path)?;
            }
        }
    }
    if link.exists(&copy.home) {
        link.remove_dir_all(&copy.home)?;
    }
    link.create_dir_all(&copy.home)?;
    let folder = link.upload(program.folder(), &copy.home)?;
    link.write(&copy.marker, &[])?;
    tracing::info!(extension = %program.manifest().id, "extension uploaded to its host");
    Ok(folder)
}

impl LanguageTools {
    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the host is not connected, the extension cannot be uploaded
    /// there, its server runtime is missing on the host, it has no build for the host's platform or
    /// a bundled path leaves its folder.
    #[tracing::instrument(skip_all, fields(extension = %program.manifest().id, local = link.is_local()))]
    pub fn program_site(
        &self,
        link: &HostLink,
        platform: Option<HostPlatform>,
        program: &TrustedProgram,
        chosen: &ServerRuntimePaths,
    ) -> Result<ProgramSite, ExtensionError> {
        let HostLink::Remote(remote) = link else {
            return Ok(ProgramSite {
                launch: self.program_launch(program, chosen)?,
                folder: program.folder().to_path_buf(),
            });
        };
        let layout = remote.layout().ok_or_else(not_connected)?;
        let platform: Platform = platform.ok_or_else(not_connected)?.into();
        let manifest = program.manifest();
        let declared = manifest
            .program
            .as_ref()
            .ok_or_else(|| ExtensionError::Missing(manifest.name.clone()))?;
        let extensions = layout.app_dir().join(EXTENSIONS_DIR);
        let launch = match &declared.launch {
            Launch::Runtime { runtime, args } => {
                let folder = upload_extension(link, &extensions, program)?;
                let id = ServerRuntimeId::new(runtime.as_ref());
                let unconstrained = RuntimeRequirements::new();
                let finder = self.host_server_runtimes(link, &unconstrained);
                let found = finder
                    .find(&id)
                    .ok_or_else(|| ExtensionError::MissingServerRuntime(id.clone()))?;
                let env = finder.launch_env(&[(id, found.clone())]);
                return Ok(ProgramSite {
                    launch: ServerLaunch {
                        program: found,
                        args: args.clone(),
                        env,
                    },
                    folder,
                });
            }
            Launch::Binary { binary, args } => {
                let build = binary
                    .iter()
                    .find(|build| platform.matches(&build.targets))
                    .ok_or(ExtensionError::NoBuild)?;
                if let BinarySource::Bundled { path } = &build.source {
                    contained(program.folder(), path)?;
                }
                let folder = upload_extension(link, &extensions, program)?;
                let executable = match &build.source {
                    BinarySource::Bundled { path } => folder.join(path.as_path()),
                    BinarySource::Download { url, sha256 } => {
                        let local = self.downloaded_binary(program, url, sha256, platform.exe())?;
                        let into = folder.join(HOST_PROGRAMS_DIR).join(sha256.as_ref());
                        link.create_dir_all(&into)?;
                        link.upload(&local, &into)?
                    }
                };
                (folder, executable, args.clone())
            }
        };
        let (folder, program, args) = launch;
        Ok(ProgramSite {
            launch: ServerLaunch {
                program,
                args,
                env: Vec::new(),
            },
            folder,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::{inspect, Inspected};
    use krad_domain::{ExtensionEntry, ExtensionSource};
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

    fn trusted_in(folder: &Path) -> Result<TrustedProgram, Box<dyn std::error::Error>> {
        let entry = ExtensionEntry {
            id: "docker".parse()?,
            source: ExtensionSource::Folder(folder.to_path_buf()),
            enabled: true,
            trusted: None,
        };
        let Inspected { fingerprint, .. } = inspect(folder, entry.clone())?;
        let trusted = ExtensionEntry {
            trusted: Some(fingerprint),
            ..entry
        };
        let inspected = inspect(folder, trusted)?;
        let extension = inspected.trusted().ok_or("the extension is trusted")?;
        Ok(extension
            .program()
            .ok_or("the extension brings a program")?)
    }

    fn extension_in(root: &Path, main: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let folder = root.join("krad-docker");
        fs::create_dir_all(&folder)?;
        fs::write(folder.join(MANIFEST_FILE), MANIFEST)?;
        fs::write(folder.join("main.js"), main)?;
        Ok(folder)
    }

    #[test]
    fn a_host_copy_lives_under_the_extension_and_its_whole_fingerprint() -> TestResult {
        let root = scratch("layout")?;
        let program = trusted_in(&extension_in(&root, "first")?)?;
        let copy = host_copy(Path::new("/home/dev/.krad-ade/extensions"), &program);
        let home =
            PathBuf::from("/home/dev/.krad-ade/extensions/docker").join(&program.fingerprint().0);
        assert_eq!(copy.folder, home.join("krad-docker"));
        assert_eq!(copy.marker, home.join(UPLOADED_MARKER));
        assert_eq!(copy.home, home);
        Ok(())
    }

    #[test]
    fn changed_code_is_uploaded_again_and_the_old_copy_removed() -> TestResult {
        let root = scratch("upload")?;
        let host = root.join("host");
        let folder = extension_in(&root, "first")?;
        let first = upload_extension(&HostLink::Local, &host, &trusted_in(&folder)?)?;
        assert_eq!(fs::read_to_string(first.join("main.js"))?, "first");
        fs::write(folder.join("main.js"), "second")?;
        let updated = upload_extension(&HostLink::Local, &host, &trusted_in(&folder)?)?;
        assert_ne!(updated, first);
        assert_eq!(fs::read_to_string(updated.join("main.js"))?, "second");
        assert!(!first.exists(), "the copy of the older code is removed");
        Ok(())
    }

    #[test]
    fn removing_old_copies_never_touches_other_extensions() -> TestResult {
        let root = scratch("siblings")?;
        let host = root.join("host");
        let other = host.join("other-extension");
        fs::create_dir_all(&other)?;
        fs::write(other.join("keep"), "keep")?;
        upload_extension(
            &HostLink::Local,
            &host,
            &trusted_in(&extension_in(&root, "x")?)?,
        )?;
        assert!(other.join("keep").exists());
        Ok(())
    }
}
