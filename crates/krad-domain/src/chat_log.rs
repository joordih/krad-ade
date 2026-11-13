mod change;
mod fold;
mod lines;
mod origin;
mod shown_output;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::chat::{AskOutcome, ToolCall, ToolUseId};
use crate::checklist::Checklist;
use crate::mention::Mention;
use crate::rewind::RewindPoint;

pub use change::ChatLogChange;
pub use fold::{fold_entry, fold_prompt, last_permission_mode};
pub use lines::conversation_lines;
pub use origin::{ChatOpener, ChatOrigin};
pub use shown_output::ShownOutput;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerPart {
    Thinking(String),
    Tool(ToolCall),
    Text(String),
    Ask(AskOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub speaker: Speaker,
    pub text: String,
    pub parts: Vec<AnswerPart>,
    pub error: Option<String>,
    pub running: bool,
    pub mentions: Vec<Mention>,
    pub checklist: Option<Checklist>,
    pub rewind: Option<RewindPoint>,
    pub tool_outputs: HashMap<ToolUseId, ShownOutput>,
}

impl ChatMessage {
    #[must_use]
    pub fn new(speaker: Speaker, text: String) -> Self {
        Self {
            speaker,
            text,
            parts: Vec::new(),
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind: None,
            tool_outputs: HashMap::new(),
        }
    }

    pub fn tools(&self) -> impl Iterator<Item = &ToolCall> {
        self.parts.iter().filter_map(|part| match part {
            AnswerPart::Tool(call) => Some(call),
            AnswerPart::Thinking(_) | AnswerPart::Text(_) | AnswerPart::Ask(_) => None,
        })
    }
}
