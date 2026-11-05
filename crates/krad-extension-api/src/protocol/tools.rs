use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::id::ToolName;
use crate::protocol::request;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CallToolParams {
    pub tool: ToolName,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolContent {
    Text { text: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    #[default]
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub outcome: ToolOutcome,
}

request!(
    CallTool,
    "tool/call",
    Program,
    CallToolParams,
    CallToolResult
);
