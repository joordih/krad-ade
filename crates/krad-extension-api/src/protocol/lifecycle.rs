use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::manifest::Placement;
use crate::protocol::workspace::EnvironmentInfo;
use crate::protocol::{notification, request, Empty};
use crate::version::ApiVersion;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Views,
    Commands,
    Window,
    Terminals,
    Files,
    Workspace,
    Settings,
    Secrets,
    Tools,
    HostKinds,
    Events,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InitializeParams {
    pub app: AppInfo,
    pub api: ApiVersion,
    pub placement: Placement,
    pub extension_folder: PathBuf,
    #[serde(default)]
    pub environment: Option<EnvironmentInfo>,
    pub features: BTreeSet<Feature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InitializeResult {
    pub features: BTreeSet<Feature>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LogParams {
    pub level: LogLevel,
    pub message: String,
}

request!(
    Initialize,
    "initialize",
    Program,
    InitializeParams,
    InitializeResult
);
notification!(Initialized, "initialized", Program, Empty);
request!(Shutdown, "shutdown", Program, Empty, Empty);
notification!(Exit, "exit", Program, Empty);
notification!(Log, "window/log", App, LogParams);
