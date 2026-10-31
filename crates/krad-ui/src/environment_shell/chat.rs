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

const USER_PREFIX: &str = "> ";
const TOOL_PREFIX: &str = "• ";
const ERROR_PREFIX: &str = "Error: ";

pub(super) fn conversation_lines(messages: &[ChatMessage]) -> Vec<String> {
    messages.iter().flat_map(message_lines).collect()
}

fn message_lines(message: &ChatMessage) -> Vec<String> {
    let mut lines: Vec<String> = match message.speaker {
        Speaker::User => message
            .text
            .lines()
            .map(|line| format!("{USER_PREFIX}{line}"))
            .collect(),
        Speaker::Assistant => message.parts.iter().flat_map(part_lines).collect(),
    };
    lines.extend(
        message
            .error
            .iter()
            .map(|error| format!("{ERROR_PREFIX}{error}")),
    );
    lines
}

fn part_lines(part: &AnswerPart) -> Vec<String> {
    match part {
        AnswerPart::Text(text) => text.lines().map(str::to_owned).collect(),
        AnswerPart::Tool(call) => {
            let line = format!("{TOOL_PREFIX}{} {}", call.name, call.summary);
            vec![line.trim_end().to_owned()]
        }
        AnswerPart::Thinking(_) | AnswerPart::Ask(_) => Vec::new(),
    }
}

#[derive(Default)]
pub(super) struct ChatState {
    pub(super) suggest: Option<Suggest>,
    suggest_scroll: ScrollHandle,
    files: Option<(PathBuf, FileIndex)>,
    skills: Option<Vec<Skill>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ToolKind;

    fn message(speaker: Speaker, text: &str, parts: Vec<AnswerPart>) -> ChatMessage {
        ChatMessage {
            speaker,
            text: text.to_owned(),
            parts,
            error: None,
            running: false,
            mentions: Vec::new(),
            checklist: None,
            rewind: None,
        }
    }

    #[test]
    fn a_conversation_reads_as_prompts_answers_and_tools_without_thoughts() {
        let tool = ToolCall {
            id: None,
            name: "bash".to_owned(),
            kind: ToolKind::Shell,
            summary: "cargo test".to_owned(),
            edit: None,
        };
        let mut answer = message(
            Speaker::Assistant,
            "Ran the tests.\nAll pass.",
            vec![
                AnswerPart::Thinking("hmm".to_owned()),
                AnswerPart::Tool(tool),
                AnswerPart::Text("Ran the tests.\nAll pass.".to_owned()),
            ],
        );
        answer.error = Some("turn stopped".to_owned());
        let lines = conversation_lines(&[
            message(Speaker::User, "Fix it\nquickly", Vec::new()),
            answer,
        ]);
        assert_eq!(
            lines,
            [
                "> Fix it",
                "> quickly",
                "• bash cargo test",
                "Ran the tests.",
                "All pass.",
                "Error: turn stopped",
            ]
        );
    }
}
