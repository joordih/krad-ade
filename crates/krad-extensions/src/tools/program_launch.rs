use krad_domain::{ServerLaunch, ServerRuntimeId, ServerRuntimePaths};
use krad_extension_api::manifest::{Binary, BinarySource, Launch};
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::LanguageTools;
use crate::error::ExtensionError;
use crate::http::{download, Sha256Digest};
use crate::platform::mark_executable;
use crate::program::InstalledExtension;
use crate::runtime_requirements::RuntimeRequirements;

const PROGRAMS_DIR: &str = "extension-programs";
const DOWNLOAD_SUFFIX: &str = "download";
const PROGRAM_FILE: &str = "program";

impl LanguageTools {
    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the extension brings no program, its server runtime is
    /// missing, it has no build for this platform, a bundled path leaves its folder or a download
    /// fails its checksum.
    #[tracing::instrument(skip_all, fields(extension = %installed.manifest.id))]
    pub fn program_launch(
        &self,
        installed: &InstalledExtension,
        chosen: &ServerRuntimePaths,
    ) -> Result<ServerLaunch, ExtensionError> {
        let program = installed
            .manifest
            .program
            .as_ref()
            .ok_or_else(|| ExtensionError::Missing(installed.manifest.name.clone()))?;
        match &program.launch {
            Launch::Runtime { runtime, args } => {
                let id = ServerRuntimeId::new(runtime.as_ref());
                let unconstrained = RuntimeRequirements::new();
                let server_runtimes = self.server_runtimes(chosen, &unconstrained);
                let found = server_runtimes
                    .find(&id)
                    .ok_or_else(|| ExtensionError::MissingServerRuntime(id.clone()))?;
                let env = server_runtimes.launch_env(&[(id, found.clone())]);
                Ok(ServerLaunch {
                    program: found,
                    args: args.clone(),
                    env,
                })
            }
            Launch::Binary { binary, args } => {
                let build = binary
                    .iter()
                    .find(|build| self.platform.matches(&build.targets))
                    .ok_or(ExtensionError::NoBuild)?;
                Ok(ServerLaunch {
                    program: self.binary_path(installed, build)?,
                    args: args.clone(),
                    env: Vec::new(),
                })
            }
        }
    }

    fn binary_path(
        &self,
        installed: &InstalledExtension,
        build: &Binary,
    ) -> Result<PathBuf, ExtensionError> {
        match &build.source {
            BinarySource::Bundled { path } => inside(&installed.folder, path),
            BinarySource::Download { url, sha256 } => {
                let folder = self
                    .root
                    .join(PROGRAMS_DIR)
                    .join(installed.manifest.id.as_ref())
                    .join(sha256);
                let target = folder.join(format!("{PROGRAM_FILE}{}", self.platform.exe()));
                if target.is_file() {
                    return Ok(target);
                }
                fs::create_dir_all(&folder)?;
                let partial = target.with_extension(DOWNLOAD_SUFFIX);
                download(&self.agent, url, Some(&Sha256Digest::new(sha256)), &partial)?;
                fs::rename(&partial, &target)?;
                mark_executable(&target)?;
                Ok(target)
            }
        }
    }
}

fn inside(folder: &Path, relative: &Path) -> Result<PathBuf, ExtensionError> {
    let stays_inside = relative
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if stays_inside {
        Ok(folder.join(relative))
    } else {
        Err(ExtensionError::OutsideFolder(relative.to_path_buf()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundled_program_must_stay_inside_its_extension() {
        let folder = Path::new("/extensions/docker");
        assert_eq!(
            inside(folder, Path::new("bin/docker-ext")).ok(),
            Some(PathBuf::from("/extensions/docker/bin/docker-ext"))
        );
        assert!(matches!(
            inside(folder, Path::new("../other/bin")),
            Err(ExtensionError::OutsideFolder(_))
        ));
        assert!(matches!(
            inside(folder, Path::new("/usr/bin/env")),
            Err(ExtensionError::OutsideFolder(_))
        ));
    }
}
