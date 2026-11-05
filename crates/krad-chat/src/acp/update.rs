use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use super::content::ContentBlock;
use super::ids::{ModeId, SessionId, TerminalId, ToolCallId};
use super::lenient::{optional_readable_items, readable_items};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNotification {
    pub session_id: SessionId,
    pub update: SessionUpdate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "sessionUpdate", rename_all = "snake_case")]
pub enum SessionUpdate {
    UserMessageChunk(ContentChunk),
    AgentMessageChunk(ContentChunk),
    AgentThoughtChunk(ContentChunk),
    ToolCall(ToolCall),
    ToolCallUpdate(ToolCallUpdate),
    Plan(Plan),
    AvailableCommandsUpdate(AvailableCommandsUpdate),
    CurrentModeUpdate(CurrentModeUpdate),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentChunk {
    pub content: ContentBlock,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub tool_call_id: ToolCallId,
    pub title: String,
    #[serde(default)]
    pub kind: ToolKind,
    #[serde(default)]
    pub status: ToolCallStatus,
    #[serde(default, deserialize_with = "readable_items")]
    pub content: Vec<ToolCallContent>,
    #[serde(default, deserialize_with = "readable_items")]
    pub locations: Vec<ToolCallLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallUpdate {
    pub tool_call_id: ToolCallId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolCallStatus>,
    #[serde(
        default,
        deserialize_with = "optional_readable_items",
        skip_serializing_if = "Option::is_none"
    )]
    pub content: Option<Vec<ToolCallContent>>,
    #[serde(
        default,
        deserialize_with = "optional_readable_items",
        skip_serializing_if = "Option::is_none"
    )]
    pub locations: Option<Vec<ToolCallLocation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<Value>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallContent {
    Content {
        content: ContentBlock,
    },
    Diff(Diff),
    Terminal {
        #[serde(rename = "terminalId")]
        terminal_id: TerminalId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallLocation {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    #[serde(deserialize_with = "readable_items")]
    pub entries: Vec<PlanEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub priority: PlanEntryPriority,
    pub status: PlanEntryStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanEntryPriority {
    High,
    Medium,
    Low,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanEntryStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCommandsUpdate {
    #[serde(deserialize_with = "readable_items")]
    pub available_commands: Vec<AvailableCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableCommand {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<AvailableCommandInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableCommandInput {
    pub hint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentModeUpdate {
    pub current_mode_id: ModeId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_tool_call_keeps_its_readable_content_and_an_unknown_kind_reads_as_other() {
        let update: Result<SessionUpdate, _> = serde_json::from_value(json!({
            "sessionUpdate": "tool_call",
            "toolCallId": "call_001",
            "title": "Editing config",
            "kind": "teleport",
            "status": "in_progress",
            "content": [
                {"type": "diff", "path": "/home/user/project/src/config.json", "oldText": "{}", "newText": "{\"debug\": true}"},
                {"type": "hologram", "data": 1},
                {"type": "terminal", "terminalId": "term_xyz789"}
            ],
            "locations": [{"path": "/home/user/project/src/config.json", "line": 3}],
            "futureField": {"nested": true}
        }));
        let Ok(SessionUpdate::ToolCall(call)) = update else {
            unreachable!("the tool call reads: {update:?}");
        };
        assert_eq!(call.kind, ToolKind::Other);
        assert_eq!(call.status, ToolCallStatus::InProgress);
        assert_eq!(call.content.len(), 2);
        assert!(matches!(
            &call.content[1],
            ToolCallContent::Terminal { terminal_id } if terminal_id.0 == "term_xyz789"
        ));
        assert_eq!(call.locations[0].line, Some(3));
    }

    #[test]
    fn an_update_tells_apart_untouched_and_cleared_content() {
        let untouched: Result<ToolCallUpdate, _> =
            serde_json::from_value(json!({"toolCallId": "call_001", "status": "completed"}));
        let cleared: Result<ToolCallUpdate, _> =
            serde_json::from_value(json!({"toolCallId": "call_001", "content": []}));
        assert_eq!(untouched.ok().and_then(|update| update.content), None);
        assert_eq!(
            cleared.ok().and_then(|update| update.content),
            Some(Vec::new())
        );
    }
}
