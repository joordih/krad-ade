mod context;
mod live;
mod panel;
mod suggest;
mod transcript;

pub(super) use live::{LiveProcess, StartError};
pub(super) use panel::question_bubble;
pub(super) use suggest::workplace_home;
use suggest::Suggest;
pub(super) use transcript::{fold_entry, fold_prompt, last_permission_mode};

use gpui_kit::ScrollHandle;
use krad_chat::{Skill, Speaker, ToolCall};
use krad_domain::{AskOutcome, Checklist, Mention, RewindPoint};
use krad_repo::FileIndex;
use std::path::PathBuf;

use super::EnvironmentShell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AnswerPart {
    Thinking(String),
    Tool(ToolCall),
    Text(String),
    Ask(AskOutcome),
}

pub(super) struct ChatMessage {
    pub(super) speaker: Speaker,
    pub(super) text: String,
    pub(super) parts: Vec<AnswerPart>,
    pub(super) error: Option<String>,
    pub(super) running: bool,
    pub(super) mentions: Vec<Mention>,
    pub(super) checklist: Option<Checklist>,
    pub(super) rewind: Option<RewindPoint>,
}

impl ChatMessage {
    pub(super) fn tools(&self) -> impl Iterator<Item = &ToolCall> {
        self.parts.iter().filter_map(|part| match part {
            AnswerPart::Tool(call) => Some(call),
            AnswerPart::Thinking(_) | AnswerPart::Text(_) | AnswerPart::Ask(_) => None,
        })
    }
}

#[derive(Default)]
pub(super) struct ChatState {
    pub(super) suggest: Option<Suggest>,
    suggest_scroll: ScrollHandle,
    files: Option<(PathBuf, FileIndex)>,
    skills: Option<Vec<Skill>>,
}
