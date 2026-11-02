use std::env::consts::{ARCH, EXE_SUFFIX, OS};
use std::fs;
use std::io;
use std::path::Path;

const WINDOWS_SCRIPT_SUFFIX: &str = ".cmd";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Os {
    Macos,
    Linux,
    Windows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arch {
    Aarch64,
    X86_64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Platform {
    pub(crate) os: Os,
    pub(crate) arch: Arch,
}

impl Platform {
    pub(crate) fn current() -> Option<Self> {
        let os = match OS {
            "macos" => Os::Macos,
            "linux" => Os::Linux,
            "windows" => Os::Windows,
            _ => return None,
        };
        let arch = match ARCH {
            "aarch64" => Arch::Aarch64,
            "x86_64" => Arch::X86_64,
            _ => return None,
        };
        Some(Self { os, arch })
    }

    pub(crate) const fn targets(self) -> &'static [&'static str] {
        match (self.os, self.arch) {
            (Os::Macos, Arch::Aarch64) => &["darwin_arm64", "darwin", "unix"],
            (Os::Macos, Arch::X86_64) => &["darwin_x64", "darwin", "unix"],
            (Os::Linux, Arch::Aarch64) => &["linux_arm64_gnu", "linux_arm64", "linux", "unix"],
            (Os::Linux, Arch::X86_64) => &["linux_x64_gnu", "linux_x64", "linux", "unix"],
            (Os::Windows, Arch::Aarch64) => &["win_arm64", "win"],
            (Os::Windows, Arch::X86_64) => &["win_x64", "win"],
        }
    }

    pub(crate) fn matches(self, targets: &[String]) -> bool {
        targets
            .iter()
            .any(|target| self.targets().contains(&target.as_str()))
    }

    pub(crate) const fn exe(self) -> &'static str {
        match self.os {
            Os::Windows => EXE_SUFFIX,
            Os::Macos | Os::Linux => "",
        }
    }

    pub(crate) const fn cmd(self) -> &'static str {
        match self.os {
            Os::Windows => WINDOWS_SCRIPT_SUFFIX,
            Os::Macos | Os::Linux => "",
        }
    }
}

#[cfg(unix)]
pub(crate) fn mark_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    const EXECUTABLE_MODE: u32 = 0o755;
    fs::set_permissions(path, fs::Permissions::from_mode(EXECUTABLE_MODE))
}

#[cfg(not(unix))]
pub(crate) fn mark_executable(path: &Path) -> io::Result<()> {
    fs::metadata(path).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_platform_matches_its_own_and_its_family_targets() {
        let mac = Platform {
            os: Os::Macos,
            arch: Arch::Aarch64,
        };
        assert!(mac.matches(&["darwin_arm64".to_owned()]));
        assert!(mac.matches(&["linux".to_owned(), "darwin".to_owned()]));
        assert!(!mac.matches(&["darwin_x64".to_owned()]));
    }
}
