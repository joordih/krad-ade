use serde::{Deserialize, Serialize};

use crate::{ChatEntry, ToolUseId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mark", content = "call", rename_all = "snake_case")]
pub enum TranscriptMark {
    ToolFinished(ToolUseId),
    Interrupted,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptChunk {
    pub entries: Vec<ChatEntry>,
    pub marks: Vec<TranscriptMark>,
    pub next: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentMeta {
    pub tool_use_id: ToolUseId,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub finished: bool,
}
