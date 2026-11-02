mod activation;
mod choice;
mod project_folder;
mod rank;
mod version;

pub use activation::{GroupActivation, ToolchainActivation};
pub use choice::{
    EffectiveToolchain, ToolchainChoice, ToolchainChoices, ToolchainScope, ToolchainSelection,
};
pub use project_folder::{NotInsideTheGroup, ProjectFolder};
pub use rank::{rank_toolchains, DeclaredToolchain, ProjectHints, RankContext};
pub use version::{NotAVersion, ToolchainVersion, VersionRequirement};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into().to_ascii_lowercase())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

id_newtype!(ToolchainFamilyId);
id_newtype!(ToolchainKind);
id_newtype!(LocatorId);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum ToolchainLocation {
    Path { path: PathBuf },
    Bundled,
}

impl ToolchainLocation {
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Path { path } => Some(path),
            Self::Bundled => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Toolchain {
    pub family: ToolchainFamilyId,
    pub kind: ToolchainKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<ToolchainVersion>,
    pub location: ToolchainLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<PathBuf>,
    pub locator: LocatorId,
}

impl Toolchain {
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.location.path()
    }

    #[must_use]
    pub fn is_same(&self, other: &Self) -> bool {
        self.family == other.family && self.location == other.location
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolchains_roundtrip_as_json() -> serde_json::Result<()> {
        let toolchain = Toolchain {
            family: ToolchainFamilyId::new("Python"),
            kind: ToolchainKind::new("venv"),
            name: ".venv".to_owned(),
            version: "3.12.4".parse().ok(),
            location: ToolchainLocation::Path {
                path: PathBuf::from("/srv/app/.venv/bin/python"),
            },
            prefix: Some(PathBuf::from("/srv/app/.venv")),
            project: None,
            locator: LocatorId::new("pet"),
        };
        let json = serde_json::to_string(&toolchain)?;
        assert_eq!(serde_json::from_str::<Toolchain>(&json)?, toolchain);
        assert_eq!(toolchain.family.as_str(), "python");
        let bundled = ToolchainLocation::Bundled;
        assert_eq!(serde_json::to_string(&bundled)?, r#"{"at":"bundled"}"#);
        Ok(())
    }
}
