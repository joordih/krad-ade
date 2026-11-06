use serde::{Deserialize, Deserializer, Serialize};

use crate::agent::AgentPreset;
use crate::checklist::Checklist;
use crate::file_edit::FileEdit;
use crate::ids::ResumeId;
use crate::mention::ChatImage;
use crate::model::{lenient_model, ModelId};
use crate::permission_mode::PermissionMode;
use crate::rewind::RewindPoint;
use crate::tool_kind::ToolKind;

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
    pub kind: ToolKind,
    pub summary: String,
    #[serde(
        default,
        alias = "edit",
        deserialize_with = "one_or_many_edits",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub edits: Vec<FileEdit>,
}

impl ToolCall {
    pub fn place_edit(&mut self, edit: FileEdit) {
        match self.edits.iter_mut().find(|shown| shown.path == edit.path) {
            Some(shown) => *shown = edit,
            None => self.edits.push(edit),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SavedEdits {
    Many(Vec<FileEdit>),
    One(FileEdit),
}

fn one_or_many_edits<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<FileEdit>, D::Error> {
    Ok(match Option::<SavedEdits>::deserialize(deserializer)? {
        Some(SavedEdits::Many(edits)) => edits,
        Some(SavedEdits::One(edit)) => vec![edit],
        None => Vec::new(),
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOutput {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatEvent {
    Session(ResumeId),
    Text(String),
    TextDelta(String),
    Thinking(String),
    Model(String),
    PermissionMode(PermissionMode),
    ContextTokens(u64),
    ContextWindow(u64),
    Tool(ToolCall),
    ToolUpdated(ToolCall),
    Edited { tool: ToolUseId, edit: FileEdit },
    ToolOutput { tool: ToolUseId, output: ToolOutput },
    Checklist(Checklist),
    Prompted(RewindPoint),
    TurnStarted,
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
        #[serde(default, deserialize_with = "lenient_model")]
        model: Option<ModelId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<ChatImage>,
    },
    Event(ChatEvent),
    Asked(AskOutcome),
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
                images: Vec::new(),
            },
            ChatEntry::User {
                text: "look".to_owned(),
                preset: AgentPreset::Gateway,
                model: None,
                images: vec![ChatImage {
                    path: std::path::PathBuf::from("/shots/1.png"),
                    format: crate::mention::ImageFormat::Png,
                }],
            },
            ChatEntry::Event(ChatEvent::Tool(ToolCall {
                id: None,
                name: "Read".to_owned(),
                kind: ToolKind::Read,
                summary: "a.rs".to_owned(),
                edits: Vec::new(),
            })),
            ChatEntry::Event(ChatEvent::Checklist(Checklist {
                items: vec![crate::checklist::ChecklistItem {
                    text: "Fix it".to_owned(),
                    status: crate::checklist::ChecklistStatus::Pending,
                }],
            })),
            ChatEntry::Event(ChatEvent::ToolOutput {
                tool: ToolUseId::from("t1"),
                output: ToolOutput {
                    text: "ok\n".to_owned(),
                    truncated: true,
                    exit_code: Some(0),
                    signal: None,
                },
            }),
            ChatEntry::Event(ChatEvent::Done),
        ];
        for entry in entries {
            let line = serde_json::to_string(&entry).unwrap_or_default();
            assert_eq!(serde_json::from_str::<ChatEntry>(&line).ok(), Some(entry));
        }
    }

    #[test]
    fn a_user_entry_saved_before_images_still_loads() {
        let line = r#"{"User":{"text":"hi","preset":"ClaudeCode","model":null}}"#;
        assert!(matches!(
            serde_json::from_str::<ChatEntry>(line),
            Ok(ChatEntry::User { images, .. }) if images.is_empty()
        ));
    }

    #[test]
    fn a_tool_saved_with_one_edit_loads_with_it_as_its_only_edit() {
        let line = r#"{"Event":{"Tool":{"name":"Edit","kind":"Edit","summary":"a.rs","edit":{"path":"a.rs","lines":[]}}}}"#;
        let edits = match serde_json::from_str::<ChatEntry>(line) {
            Ok(ChatEntry::Event(ChatEvent::Tool(call))) => call.edits,
            _ => Vec::new(),
        };
        assert_eq!(
            edits,
            [FileEdit {
                path: "a.rs".to_owned(),
                lines: Vec::new()
            }]
        );
    }

    #[test]
    fn placing_an_edit_replaces_the_one_for_its_path_and_adds_new_paths() {
        let edit = |path: &str, lines: Vec<crate::file_edit::EditLine>| FileEdit {
            path: path.to_owned(),
            lines,
        };
        let mut call = ToolCall {
            id: None,
            name: "Edit".to_owned(),
            kind: ToolKind::Edit,
            summary: String::new(),
            edits: vec![edit("a.rs", Vec::new())],
        };
        let changed = crate::line_diff("x", "y");
        call.place_edit(edit("a.rs", changed.clone()));
        call.place_edit(edit("b.rs", Vec::new()));
        assert_eq!(
            call.edits,
            [edit("a.rs", changed), edit("b.rs", Vec::new())]
        );
    }
}
