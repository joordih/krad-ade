use krad_domain::{LocatorId, ToolchainFamilyId, ToolchainKind};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
pub struct ToolchainFamilyManifest {
    pub id: ToolchainFamilyId,
    pub name: String,
    pub languages: Vec<String>,
    #[serde(default)]
    pub markers: Vec<String>,
    #[serde(default)]
    pub priority: Vec<ToolchainKind>,
    #[serde(default)]
    pub kinds: Vec<KindLabel>,
    #[serde(default)]
    pub locators: Vec<LocatorSpec>,
    #[serde(default)]
    pub hints: Vec<HintSpec>,
    #[serde(default)]
    pub activation: Vec<Activation>,
    #[serde(default)]
    pub entered: Vec<GlobLocator>,
}

impl ToolchainFamilyManifest {
    #[must_use]
    pub fn label(&self, kind: &ToolchainKind) -> String {
        self.kinds
            .iter()
            .find(|label| label.id == *kind)
            .map_or_else(|| kind.to_string(), |label| label.label.clone())
    }

    #[must_use]
    pub fn covers(&self, language: &str) -> bool {
        self.languages.iter().any(|known| known == language)
    }

    #[must_use]
    pub fn activation(&self, kind: &ToolchainKind) -> Option<&Activation> {
        self.activation
            .iter()
            .find(|activation| activation.kinds.contains(kind))
            .or_else(|| {
                self.activation
                    .iter()
                    .find(|activation| activation.kinds.is_empty())
            })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct KindLabel {
    pub id: ToolchainKind,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Activation {
    #[serde(default)]
    pub kinds: Vec<ToolchainKind>,
    pub script: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "locate", rename_all = "snake_case")]
pub enum LocatorSpec {
    Pet { id: LocatorId },
    Glob(GlobLocator),
    Command(CommandLocator),
    Env(EnvLocator),
    Bundled(BundledLocator),
    Shared(SharedLocator),
}

impl LocatorSpec {
    #[must_use]
    pub fn id(&self) -> &LocatorId {
        match self {
            Self::Pet { id } => id,
            Self::Glob(locator) => &locator.id,
            Self::Command(locator) => &locator.id,
            Self::Env(locator) => &locator.id,
            Self::Bundled(locator) => &locator.id,
            Self::Shared(locator) => &locator.id,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobBase {
    #[default]
    ProjectFolder,
    GroupRoot,
    Home,
    Absolute,
    Entered,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GlobLocator {
    pub id: LocatorId,
    pub kind: ToolchainKind,
    #[serde(default)]
    pub base: GlobBase,
    pub patterns: Vec<String>,
    #[serde(default)]
    pub require: Vec<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<VersionSource>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum VersionSource {
    Command {
        #[serde(default)]
        program: Option<String>,
        args: Vec<String>,
        #[serde(default)]
        after: Option<String>,
    },
    JsonFile {
        file: String,
        pointer: String,
    },
    TextFile {
        file: String,
    },
    Name,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum CommandOutput {
    Lines {
        #[serde(default)]
        skip_prefix: Vec<String>,
    },
    Json {
        #[serde(default)]
        items: String,
        path: String,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct CommandLocator {
    pub id: LocatorId,
    pub kind: ToolchainKind,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub output: CommandOutput,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<VersionSource>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EnvLocator {
    pub id: LocatorId,
    pub kind: ToolchainKind,
    pub var: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<VersionSource>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BundledLocator {
    pub id: LocatorId,
    pub kind: ToolchainKind,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SharedLocator {
    pub id: LocatorId,
    pub uses: LocatorId,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "locate", rename_all = "snake_case")]
pub enum SharedLocatorManifest {
    Glob(GlobLocator),
    Command(CommandLocator),
}

impl SharedLocatorManifest {
    #[must_use]
    pub fn id(&self) -> &LocatorId {
        match self {
            Self::Glob(locator) => &locator.id,
            Self::Command(locator) => &locator.id,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum HintFormat {
    VersionText,
    ToolVersions { tool: String },
    PackageJsonEngine { engine: String },
    RustToolchain,
    EnvironmentName,
    PyrightVenv,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HintSpec {
    pub file: String,
    #[serde(flatten)]
    pub format: HintFormat,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffApply {
    #[default]
    Live,
    Restart,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectConfig {
    pub file: String,
    #[serde(default)]
    pub table: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerDefault {
    pub key: String,
    pub value: toml::Value,
    #[serde(default)]
    pub unless_project_config: Vec<ProjectConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerToolchain {
    pub family: ToolchainFamilyId,
    #[serde(default)]
    pub apply: HandoffApply,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub path: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub defaults: Vec<ServerDefault>,
}
