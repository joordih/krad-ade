use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::id::{CommandId, QualifiedCommand, TerminalHandle};
use crate::protocol::{request, Empty};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandTarget {
    File {
        path: PathBuf,
    },
    Folder {
        path: PathBuf,
    },
    Tab {
        title: String,
        terminal: Option<TerminalHandle>,
    },
    Group {
        name: String,
        root: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteCommandParams {
    pub command: CommandId,
    #[serde(default)]
    pub target: Option<CommandTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RunCommandParams {
    pub command: QualifiedCommand,
    #[serde(default)]
    pub target: Option<CommandTarget>,
}

request!(
    ExecuteCommand,
    "command/execute",
    Program,
    ExecuteCommandParams,
    Empty
);
request!(RunCommand, "command/run", App, RunCommandParams, Empty);
