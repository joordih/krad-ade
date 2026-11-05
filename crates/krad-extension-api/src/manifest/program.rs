use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::id::RuntimeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    App,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Program {
    pub placement: Placement,
    #[serde(flatten)]
    pub launch: Launch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Launch {
    Runtime {
        runtime: RuntimeId,
        #[serde(default)]
        args: Vec<String>,
    },
    Binary {
        binary: Vec<Binary>,
        #[serde(default)]
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Binary {
    pub targets: Vec<String>,
    #[serde(flatten)]
    pub source: BinarySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BinarySource {
    Bundled { path: PathBuf },
    Download { url: String, sha256: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    Startup,
    EnvironmentOpened,
    WorkspaceContains(String),
}
