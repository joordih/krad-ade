use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::str::FromStr;

use serde::{Deserialize, Deserializer};

use crate::signature::ReleaseSignature;
use crate::version::AppVersion;

const PATH_SEPARATORS: [char; 2] = ['/', '\\'];
const HIDDEN_PREFIX: char = '.';
const TARGET_SEPARATOR: char = '-';
const AARCH64: &str = "aarch64";
const X86_64: &str = "x86_64";
const MACOS: &str = "macos";
const WINDOWS: &str = "windows";
const LINUX: &str = "linux";

#[derive(Debug, Deserialize)]
pub(crate) struct ReleaseManifest {
    pub(crate) version: AppVersion,
    #[serde(default)]
    pub(crate) notes: String,
    #[serde(default, deserialize_with = "known_targets")]
    builds: HashMap<ReleaseTarget, ReleaseBuild>,
    #[serde(default, deserialize_with = "known_targets")]
    targets: HashMap<ReleaseTarget, ReleaseBuild>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReleaseBuild {
    pub(crate) file: AssetName,
    pub(crate) size: u64,
    pub(crate) signature: ReleaseSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ReleaseTarget {
    MacosAarch64,
    MacosX86_64,
    WindowsX86_64,
    LinuxX86_64,
    LinuxAarch64,
}

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a release target")]
pub(crate) struct ReleaseTargetError(String);

impl ReleaseManifest {
    pub(crate) fn build(&self, target: ReleaseTarget) -> Option<&ReleaseBuild> {
        self.targets
            .get(&target)
            .or_else(|| self.builds.get(&target))
    }
}

impl ReleaseTarget {
    const ALL: [Self; 5] = [
        Self::MacosAarch64,
        Self::MacosX86_64,
        Self::WindowsX86_64,
        Self::LinuxX86_64,
        Self::LinuxAarch64,
    ];

    const fn arch(self) -> &'static str {
        match self {
            Self::MacosAarch64 | Self::LinuxAarch64 => AARCH64,
            Self::MacosX86_64 | Self::WindowsX86_64 | Self::LinuxX86_64 => X86_64,
        }
    }

    const fn os(self) -> &'static str {
        match self {
            Self::MacosAarch64 | Self::MacosX86_64 => MACOS,
            Self::WindowsX86_64 => WINDOWS,
            Self::LinuxX86_64 | Self::LinuxAarch64 => LINUX,
        }
    }

    fn for_platform(arch: &str, os: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|target| target.arch() == arch && target.os() == os)
    }

    pub(crate) fn running() -> Option<Self> {
        Self::for_platform(ARCH, OS)
    }
}

impl FromStr for ReleaseTarget {
    type Err = ReleaseTargetError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        key.split_once(TARGET_SEPARATOR)
            .and_then(|(arch, os)| Self::for_platform(arch, os))
            .ok_or_else(|| ReleaseTargetError(key.to_owned()))
    }
}

fn known_targets<'de, D>(deserializer: D) -> Result<HashMap<ReleaseTarget, ReleaseBuild>, D::Error>
where
    D: Deserializer<'de>,
{
    let builds = HashMap::<String, ReleaseBuild>::deserialize(deserializer)?;
    Ok(builds
        .into_iter()
        .filter_map(|(key, build)| Some((key.parse().ok()?, build)))
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct AssetName(String);

#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a plain release file name")]
pub(crate) struct AssetNameError(String);

impl TryFrom<String> for AssetName {
    type Error = AssetNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        let plain =
            !name.is_empty() && !name.starts_with(HIDDEN_PREFIX) && !name.contains(PATH_SEPARATORS);
        if plain {
            Ok(Self(name))
        } else {
            Err(AssetNameError(name))
        }
    }
}

impl AsRef<str> for AssetName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::tests::SIGNATURE;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn manifest_with(file: &str) -> String {
        serde_json::json!({
            "version": "0.2.0",
            "notes": "Faster chats",
            "builds": {
                "aarch64-macos": { "file": file, "size": 5, "signature": SIGNATURE }
            }
        })
        .to_string()
    }

    fn build(file: &str) -> serde_json::Value {
        serde_json::json!({ "file": file, "size": 5, "signature": SIGNATURE })
    }

    #[test]
    fn reads_the_builds_of_a_release() -> TestResult {
        let manifest: ReleaseManifest =
            serde_json::from_str(&manifest_with("krad-ade-0.2.0-aarch64-macos.app.tar.gz"))?;
        assert_eq!(manifest.version, "0.2.0".parse()?);
        let build = manifest
            .build(ReleaseTarget::MacosAarch64)
            .ok_or("no macOS build")?;
        assert_eq!(
            build.file.as_ref(),
            "krad-ade-0.2.0-aarch64-macos.app.tar.gz"
        );
        assert_eq!(build.size, 5);
        Ok(())
    }

    #[test]
    fn picks_the_build_of_each_architecture() -> TestResult {
        let manifest: ReleaseManifest = serde_json::from_value(serde_json::json!({
            "version": "0.4.0",
            "builds": {
                "aarch64-macos": build("legacy-aarch64-macos.app.tar.gz"),
                "x86_64-windows": build("legacy-setup.exe"),
                "x86_64-linux": build("legacy-x86_64-linux.tar.gz")
            },
            "targets": {
                "aarch64-macos": build("aarch64-macos.app.tar.gz"),
                "x86_64-macos": build("x86_64-macos.app.tar.gz"),
                "x86_64-windows": build("setup.exe"),
                "x86_64-linux": build("x86_64-linux.tar.gz"),
                "aarch64-linux": build("aarch64-linux.tar.gz")
            }
        }))?;
        let expected = [
            (ReleaseTarget::MacosAarch64, "aarch64-macos.app.tar.gz"),
            (ReleaseTarget::MacosX86_64, "x86_64-macos.app.tar.gz"),
            (ReleaseTarget::WindowsX86_64, "setup.exe"),
            (ReleaseTarget::LinuxX86_64, "x86_64-linux.tar.gz"),
            (ReleaseTarget::LinuxAarch64, "aarch64-linux.tar.gz"),
        ];
        for (target, file) in expected {
            let picked = manifest.build(target).ok_or("no build picked")?;
            assert_eq!(picked.file.as_ref(), file, "{target:?}");
        }
        Ok(())
    }

    #[test]
    fn a_target_only_in_the_legacy_builds_still_resolves() -> TestResult {
        let manifest: ReleaseManifest = serde_json::from_value(serde_json::json!({
            "version": "0.4.0",
            "builds": { "x86_64-linux": build("x86_64-linux.tar.gz") },
            "targets": { "aarch64-linux": build("aarch64-linux.tar.gz") }
        }))?;
        let linux = manifest
            .build(ReleaseTarget::LinuxX86_64)
            .ok_or("no x86_64 Linux build")?;
        assert_eq!(linux.file.as_ref(), "x86_64-linux.tar.gz");
        assert!(manifest.build(ReleaseTarget::MacosX86_64).is_none());
        Ok(())
    }

    #[test]
    fn ignores_targets_this_version_does_not_know() -> TestResult {
        let manifest: ReleaseManifest = serde_json::from_value(serde_json::json!({
            "version": "0.4.0",
            "targets": {
                "riscv64-linux": build("riscv64-linux.tar.gz"),
                "aarch64-windows": build("arm-setup.exe"),
                "aarch64-linux": build("aarch64-linux.tar.gz")
            }
        }))?;
        assert!(manifest.build(ReleaseTarget::LinuxAarch64).is_some());
        assert!(manifest.build(ReleaseTarget::WindowsX86_64).is_none());
        Ok(())
    }

    #[test]
    fn maps_the_running_architecture_and_system_to_a_target() {
        let cases = [
            ("aarch64", "macos", Some(ReleaseTarget::MacosAarch64)),
            ("x86_64", "macos", Some(ReleaseTarget::MacosX86_64)),
            ("x86_64", "windows", Some(ReleaseTarget::WindowsX86_64)),
            ("x86_64", "linux", Some(ReleaseTarget::LinuxX86_64)),
            ("aarch64", "linux", Some(ReleaseTarget::LinuxAarch64)),
            ("aarch64", "windows", None),
            ("riscv64", "linux", None),
        ];
        for (arch, os, target) in cases {
            assert_eq!(ReleaseTarget::for_platform(arch, os), target, "{arch}-{os}");
        }
    }

    #[test]
    fn parses_the_manifest_keys() -> TestResult {
        assert_eq!(
            "x86_64-macos".parse::<ReleaseTarget>()?,
            ReleaseTarget::MacosX86_64
        );
        assert_eq!(
            "aarch64-linux".parse::<ReleaseTarget>()?,
            ReleaseTarget::LinuxAarch64
        );
        for unknown in ["", "x86_64", "macos-aarch64", "x86_64-macos-extra"] {
            assert!(
                unknown.parse::<ReleaseTarget>().is_err(),
                "{unknown} parsed"
            );
        }
        Ok(())
    }

    #[test]
    fn refuses_file_names_that_leave_the_staging_folder() {
        for file in ["../evil", "a/b", "a\\b", ".hidden", ""] {
            assert!(
                serde_json::from_str::<ReleaseManifest>(&manifest_with(file)).is_err(),
                "{file} accepted"
            );
        }
    }
}
