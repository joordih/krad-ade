mod context;
mod engine;
mod live;
mod panel;
mod suggest;
mod transcript;

pub(super) use live::{LiveProcess, StartError};
pub(super) use panel::question_bubble;
use suggest::Suggest;
pub(super) use suggest::{workplace_home, Composer};
pub(super) use transcript::{fold_entry, fold_prompt, last_permission_mode};

use gpui_kit::component::input::TextareaState;
use gpui_kit::{Entity, ScrollHandle, Subscription};
use krad_chat::{Skill, Speaker, ToolCall};
use krad_domain::{
    AgentPreset, AskOutcome, ChatId, Checklist, Mention, PermissionMode, ResumeId, RewindPoint,
};
use krad_repo::FileIndex;
use krad_store::ChatLogWriter;
use std::path::PathBuf;

use super::ask::AskQueue;
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
    pub(super) tools: Vec<ToolCall>,
    pub(super) parts: Vec<AnswerPart>,
    pub(super) preset: AgentPreset,
    pub(super) error: Option<String>,
    pub(super) running: bool,
    pub(super) mentions: Vec<Mention>,
    pub(super) checklist: Option<Checklist>,
    pub(super) rewind: Option<RewindPoint>,
}

#[derive(Default)]
pub(super) struct ChatState {
    pub(super) messages: Vec<ChatMessage>,
    input: Option<(Entity<TextareaState>, Subscription)>,
    pub(super) suggest: Option<Suggest>,
    suggest_scroll: ScrollHandle,
    files: Option<(PathBuf, FileIndex)>,
    skills: Option<Vec<Skill>>,
    id: Option<ChatId>,
    log: Option<ChatLogWriter>,
    resume: Option<ResumeId>,
    pub(super) live: Option<LiveProcess>,
    pub(super) asks: AskQueue,
    pub(super) permission_mode: Option<PermissionMode>,
}

impl ChatState {
    pub(super) fn input(&self) -> Option<&Entity<TextareaState>> {
        self.input.as_ref().map(|(input, _)| input)
    }
}
