use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::agent::AgentPreset;
use crate::file_edit::FileEdit;
use crate::ids::{InvalidResumeId, ResumeId};
use crate::model::ModelId;
use crate::permission_mode::PermissionMode;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolUseId(String);

impl ToolUseId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ToolUseId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ToolUseId>,
    pub name: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<FileEdit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatEvent {
    Session(ResumeId),
    Text(String),
    Thinking(String),
    Model(String),
    PermissionMode(PermissionMode),
    ContextTokens(u64),
    ContextWindow(u64),
    Tool(ToolCall),
    Edited { tool: ToolUseId, edit: FileEdit },
    Done,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AskVerdict {
    Allowed,
    AllowedAlways,
    Denied,
    Answered,
    PlanApproved(PermissionMode),
    PlanRejected,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskOutcome {
    pub subject: String,
    pub verdict: AskVerdict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatEntry {
    User {
        text: String,
        preset: AgentPreset,
        model: Option<ModelId>,
    },
    Event(ChatEvent),
    Asked(AskOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChatId(ResumeId);

impl ChatId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<ResumeId> for ChatId {
    fn from(id: ResumeId) -> Self {
        Self(id)
    }
}

impl FromStr for ChatId {
    type Err = InvalidResumeId;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        raw.parse().map(Self)
    }
}

impl fmt::Display for ChatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_through_json() {
        let entries = [
            ChatEntry::User {
                text: "hi".to_owned(),
                preset: AgentPreset::ClaudeCode,
                model: ModelId::parse("opus"),
            },
            ChatEntry::Event(ChatEvent::Tool(ToolCall {
                id: None,
                name: "Read".to_owned(),
                summary: "a.rs".to_owned(),
                edit: None,
            })),
            ChatEntry::Event(ChatEvent::Done),
        ];
        for entry in entries {
            let line = serde_json::to_string(&entry).unwrap_or_default();
            assert_eq!(serde_json::from_str::<ChatEntry>(&line).ok(), Some(entry));
        }
    }

    #[test]
    fn chat_ids_refuse_path_characters() {
        assert!("../etc".parse::<ChatId>().is_err());
        assert_eq!(
            "0b7c3a52-9f1e".parse::<ChatId>().map(|id| id.to_string()),
            Ok("0b7c3a52-9f1e".to_owned())
        );
    }
}
