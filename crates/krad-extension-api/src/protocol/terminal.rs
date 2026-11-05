use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::id::TerminalHandle;
use crate::protocol::{notification, request};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalZone {
    #[default]
    Session,
    Utilities,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TerminalCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenTerminalParams {
    pub title: String,
    #[serde(default)]
    pub command: Option<TerminalCommand>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub zone: TerminalZone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SendTerminalParams {
    pub terminal: TerminalHandle,
    pub text: String,
}

request!(
    OpenTerminal,
    "terminal/open",
    App,
    OpenTerminalParams,
    TerminalHandle
);
notification!(SendTerminal, "terminal/send", App, SendTerminalParams);
