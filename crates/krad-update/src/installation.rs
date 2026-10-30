#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos;
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod unix;
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
mod unsupported;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod windows;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use linux as platform;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use macos as platform;
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
use unsupported as platform;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use windows as platform;

use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use crate::error::UpdateError;
use crate::manifest::ReleaseTarget;

const MARKER_FILE: &str = "krad-ade.install";
const PROBE_FILE: &str = ".krad-ade-update-probe";

#[derive(Clone, Debug)]
pub struct Installation {
    pub(crate) root: PathBuf,
    pub(crate) staging: PathBuf,
    pub(crate) target: ReleaseTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Replaceable,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterInstall {
    Relaunch,
    Exit,
}

fn probe(dir: &Path) -> io::Result<()> {
    let probe = dir.join(PROBE_FILE);
    File::create(&probe)?;
    fs::remove_file(probe)
}

const fn is_read_only(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
    )
}

impl Installation {
    /// # Errors
    ///
    /// [`UpdateError::Io`] when the running executable can't be located.
    pub fn detect() -> Result<Option<Self>, UpdateError> {
        let exe = std::env::current_exe()?;
        let installed = platform::locate(&exe).filter(|installation| {
            platform::marker_dir(installation)
                .join(MARKER_FILE)
                .is_file()
        });
        Ok(installed)
    }

    /// # Errors
    ///
    /// [`UpdateError::Io`] when writing next to the installation fails for a
    /// reason other than missing permissions.
    pub fn reach(&self) -> Result<Reach, UpdateError> {
        let checked = fs::create_dir_all(&self.staging).and_then(|()| {
            platform::writable_dirs(self)
                .into_iter()
                .try_for_each(probe)
        });
        match checked {
            Ok(()) => Ok(Reach::Replaceable),
            Err(error) if is_read_only(error.kind()) => Ok(Reach::ReadOnly),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn fresh_staging(&self) -> io::Result<&Path> {
        if self.staging.exists() {
            fs::remove_dir_all(&self.staging)?;
        }
        fs::create_dir_all(&self.staging)?;
        Ok(&self.staging)
    }

    pub(crate) fn unpack(&self, archive: &Path) -> Result<PathBuf, UpdateError> {
        platform::unpack(archive, &self.staging)
    }

    pub(crate) fn install(&self, payload: &Path, after: AfterInstall) -> Result<(), UpdateError> {
        platform::install(self, payload, after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_writable_folder_is_replaceable_and_keeps_no_probe() -> TestResult {
        let dir = std::env::temp_dir().join(format!("krad-update-reach-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        probe(&dir)?;
        assert!(!dir.join(PROBE_FILE).exists());
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn finds_the_bundle_around_the_executable() -> TestResult {
        let exe = Path::new("/Applications/Krad ADE.app/Contents/MacOS/krad-ade");
        let installation = platform::locate(exe).ok_or("no bundle around the executable")?;
        assert_eq!(installation.root, Path::new("/Applications/Krad ADE.app"));
        assert_eq!(
            installation.staging,
            Path::new("/Applications/.krad-ade-update")
        );
        assert_eq!(
            platform::marker_dir(&installation),
            Path::new("/Applications/Krad ADE.app/Contents/Resources")
        );
        Ok(())
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn a_bare_executable_is_not_a_bundle() {
        assert!(platform::locate(Path::new("/Users/me/dev/target/release/krad-ade")).is_none());
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn swaps_the_bundle_and_keeps_the_old_one_in_staging() -> TestResult {
        let root = std::env::temp_dir().join(format!("krad-update-swap-{}", std::process::id()));
        let installation = Installation {
            root: root.join("Krad ADE.app"),
            staging: root.join(".krad-ade-update"),
            target: ReleaseTarget::MacosAarch64,
        };
        fs::create_dir_all(installation.root.join("Contents"))?;
        fs::write(installation.root.join("Contents/version"), "old")?;
        let payload = installation.staging.join("unpacked/Krad ADE.app");
        fs::create_dir_all(payload.join("Contents"))?;
        fs::write(payload.join("Contents/version"), "new")?;
        installation.install(&payload, AfterInstall::Exit)?;
        let installed = fs::read_to_string(installation.root.join("Contents/version"))?;
        assert_eq!(installed, "new");
        assert!(installation.staging.join("retired.app").is_dir());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    #[test]
    fn unpacks_the_folder_inside_a_tarball() -> TestResult {
        let root = std::env::temp_dir().join(format!("krad-update-untar-{}", std::process::id()));
        let source = root.join("source");
        let staging = root.join("staging");
        fs::create_dir_all(source.join("Krad ADE.app/Contents"))?;
        fs::write(source.join("Krad ADE.app/Contents/Info.plist"), "plist")?;
        fs::create_dir_all(&staging)?;
        let archive = root.join("app.tar.gz");
        let packed = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .arg("Krad ADE.app")
            .status()?;
        assert!(packed.success());
        let payload = unix::untar(&archive, &staging)?;
        assert!(payload.ends_with("Krad ADE.app"));
        assert!(payload.join("Contents/Info.plist").is_file());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
