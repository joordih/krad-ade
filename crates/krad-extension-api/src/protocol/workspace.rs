use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::protocol::{notification, request, Empty};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostInfo {
    Local,
    Ssh { destination: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnvironmentInfo {
    pub id: String,
    pub name: String,
    pub host: HostInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct FilePosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceContextResult {
    #[serde(default)]
    pub environment: Option<EnvironmentInfo>,
    pub groups: Vec<GroupInfo>,
    #[serde(default)]
    pub active_file: Option<PathBuf>,
    #[serde(default)]
    pub cursor: Option<FilePosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceEvent {
    EnvironmentOpened { environment: EnvironmentInfo },
    EnvironmentClosed { environment: EnvironmentInfo },
    GroupsChanged { groups: Vec<GroupInfo> },
    ActiveFileChanged { path: Option<PathBuf> },
    FileSaved { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenFileParams {
    pub path: PathBuf,
    #[serde(default)]
    pub position: Option<FilePosition>,
}

request!(
    WorkspaceContext,
    "workspace/context",
    App,
    Empty,
    WorkspaceContextResult
);
notification!(
    WorkspaceEventNotification,
    "workspace/event",
    Program,
    WorkspaceEvent
);
request!(OpenFile, "file/open", App, OpenFileParams, Empty);
