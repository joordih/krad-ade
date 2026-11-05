use krad_domain::{ServerLaunch, ServerRuntimeId, ServerRuntimePaths};
use krad_extension_api::manifest::{
    Binary, BinarySource, HttpsUrl, Launch, ProgramPath, Sha256Hex,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::LanguageTools;
use crate::error::ExtensionError;
use crate::http::{download, Sha256Digest};
use crate::platform::mark_executable;
use crate::runtime_requirements::RuntimeRequirements;
use crate::trust::TrustedProgram;

const PROGRAMS_DIR: &str = "extension-programs";
const DOWNLOAD_SUFFIX: &str = "download";
const PROGRAM_FILE: &str = "program";
const HASH_CHUNK: usize = 64 * 1024;

impl LanguageTools {
    /// # Errors
    ///
    /// Returns [`ExtensionError`] when the extension brings no program, its server runtime is
    /// missing, it has no build for this platform, a bundled path leaves its folder or a download
    /// fails its checksum.
    #[tracing::instrument(skip_all, fields(extension = %program.manifest().id))]
    pub fn program_launch(
        &self,
        program: &TrustedProgram,
        chosen: &ServerRuntimePaths,
    ) -> Result<ServerLaunch, ExtensionError> {
        let manifest = program.manifest();
        let declared = manifest
            .program
            .as_ref()
            .ok_or_else(|| ExtensionError::Missing(manifest.name.clone()))?;
        match &declared.launch {
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
                    program: self.binary_path(program, build)?,
                    args: args.clone(),
                    env: Vec::new(),
                })
            }
        }
    }

    fn binary_path(
        &self,
        program: &TrustedProgram,
        build: &Binary,
    ) -> Result<PathBuf, ExtensionError> {
        match &build.source {
            BinarySource::Bundled { path } => contained(program.folder(), path),
            BinarySource::Download { url, sha256 } => {
                self.downloaded_binary(program, url, sha256, self.platform.exe())
            }
        }
    }

    pub(super) fn downloaded_binary(
        &self,
        program: &TrustedProgram,
        url: &HttpsUrl,
        sha256: &Sha256Hex,
        exe: &str,
    ) -> Result<PathBuf, ExtensionError> {
        let folder = program
            .manifest()
            .id
            .folder_in(&self.root.join(PROGRAMS_DIR))
            .join(sha256.as_ref());
        let target = folder.join(format!("{PROGRAM_FILE}{exe}"));
        if cached(&target, sha256)? {
            return Ok(target);
        }
        fs::create_dir_all(&folder)?;
        let partial = target.with_extension(DOWNLOAD_SUFFIX);
        download(
            &self.agent,
            url.as_ref(),
            Some(&Sha256Digest::new(sha256.as_ref())),
            &partial,
        )?;
        fs::rename(&partial, &target)?;
        mark_executable(&target)?;
        Ok(target)
    }
}

fn cached(target: &Path, sha256: &Sha256Hex) -> Result<bool, ExtensionError> {
    if !target.is_file() {
        return Ok(false);
    }
    if file_digest(target)? == sha256.as_ref() {
        return Ok(true);
    }
    fs::remove_file(target)?;
    Ok(false)
}

fn file_digest(path: &Path) -> Result<String, ExtensionError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0; HASH_CHUNK];
    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn contained(folder: &Path, relative: &ProgramPath) -> Result<PathBuf, ExtensionError> {
    let outside = || ExtensionError::OutsideFolder(relative.as_path().to_path_buf());
    let joined = folder.join(relative.as_path());
    let root = fs::canonicalize(folder)?;
    let real = fs::canonicalize(&joined).map_err(|_| outside())?;
    if real.starts_with(&root) && real.is_file() {
        Ok(real)
    } else {
        Err(outside())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn folder(name: &str) -> std::io::Result<PathBuf> {
        let folder =
            std::env::temp_dir().join(format!("krad-contained-{name}-{}", std::process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }
        fs::create_dir_all(folder.join("bin"))?;
        fs::write(folder.join("bin/tool"), "#!/bin/sh")?;
        Ok(folder)
    }

    #[test]
    fn a_bundled_program_inside_its_extension_is_found() -> TestResult {
        let root = folder("inside")?;
        let found = contained(&root, &"bin/tool".parse()?)?;
        assert_eq!(found, fs::canonicalize(root.join("bin/tool"))?);
        assert!(matches!(
            contained(&root, &"bin/missing".parse()?),
            Err(ExtensionError::OutsideFolder(_))
        ));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_link_that_points_out_of_the_extension_is_refused() -> TestResult {
        let root = folder("link")?;
        std::os::unix::fs::symlink("/bin/sh", root.join("bin/escape"))?;
        assert!(matches!(
            contained(&root, &"bin/escape".parse()?),
            Err(ExtensionError::OutsideFolder(_))
        ));
        Ok(())
    }

    #[test]
    fn a_cached_download_that_no_longer_matches_its_checksum_is_dropped() -> TestResult {
        let root = folder("digest")?;
        let file = root.join("bin/tool");
        let digest: Sha256Hex = file_digest(&file)?.parse()?;
        assert!(cached(&file, &digest)?);
        fs::write(&file, "#!/bin/sh\necho swapped")?;
        assert!(!cached(&file, &digest)?);
        assert!(!file.exists());
        Ok(())
    }
}
