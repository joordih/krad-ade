use std::collections::HashMap;

use serde::Deserialize;

use crate::signature::ReleaseSignature;
use crate::version::AppVersion;

const PATH_SEPARATORS: [char; 2] = ['/', '\\'];
const HIDDEN_PREFIX: char = '.';

#[derive(Debug, Deserialize)]
pub(crate) struct ReleaseManifest {
    pub(crate) version: AppVersion,
    #[serde(default)]
    pub(crate) notes: String,
    pub(crate) builds: HashMap<ReleaseTarget, ReleaseBuild>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReleaseBuild {
    pub(crate) file: AssetName,
    pub(crate) size: u64,
    pub(crate) signature: ReleaseSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub(crate) enum ReleaseTarget {
    #[serde(rename = "aarch64-macos")]
    MacosAarch64,
    #[serde(rename = "x86_64-windows")]
    WindowsX86_64,
    #[serde(rename = "x86_64-linux")]
    LinuxX86_64,
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

    #[test]
    fn reads_the_builds_of_a_release() -> TestResult {
        let manifest: ReleaseManifest =
            serde_json::from_str(&manifest_with("krad-ade-0.2.0-aarch64-macos.app.tar.gz"))?;
        assert_eq!(manifest.version, "0.2.0".parse()?);
        let build = &manifest.builds[&ReleaseTarget::MacosAarch64];
        assert_eq!(
            build.file.as_ref(),
            "krad-ade-0.2.0-aarch64-macos.app.tar.gz"
        );
        assert_eq!(build.size, 5);
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
