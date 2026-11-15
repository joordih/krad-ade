use super::protocol::MemoryTools;
use crate::CodeTools;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EXTENSION_TOOL_PREFIX: &str = "ext_";
const NAME_SEPARATOR: char = '_';
const NAME_FILLER: char = '-';
const MAX_TOOL_NAME: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ExtensionToolEffect {
    #[default]
    ReadOnly,
    Destructive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ExtensionToolInfo {
    pub name: String,
    pub extension: String,
    pub description: String,
    pub input_schema: Value,
    pub effect: ExtensionToolEffect,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ToolCatalog {
    pub code_tools: CodeTools,
    pub extension_tools: Vec<ExtensionToolInfo>,
    #[serde(default)]
    pub memory_tools: MemoryTools,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ExtensionToolOutcome {
    #[default]
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ExtensionToolAnswer {
    pub text: String,
    pub outcome: ExtensionToolOutcome,
}

fn mcp_safe(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == NAME_FILLER {
                character
            } else {
                NAME_FILLER
            }
        })
        .collect()
}

#[must_use]
pub fn extension_tool_name(extension: &str, tool: &str) -> Option<String> {
    let name = format!(
        "{EXTENSION_TOOL_PREFIX}{}{NAME_SEPARATOR}{}",
        mcp_safe(extension),
        mcp_safe(tool)
    );
    (name.len() <= MAX_TOOL_NAME).then_some(name)
}

#[must_use]
pub fn is_extension_tool(name: &str) -> bool {
    name.starts_with(EXTENSION_TOOL_PREFIX)
}
