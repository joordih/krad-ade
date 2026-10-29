use gpui_kit::{AppContext as _, Context};
use krad_chat::{new_session_id, ChatEvent, Speaker};
use krad_domain::{
    AgentPreset, ChatEntry, ChatId, FileEdit, PermissionMode, ResumeId, ToolCall, ToolUseId,
};
use krad_store::{read_latest_chat, ChatLogWriter, StoredChat};

use super::{AnswerPart, ChatMessage, EnvironmentShell};

const BLOCK_SEPARATOR: &str = "\n\n";

const fn joins_whole_blocks(preset: AgentPreset) -> bool {
    !matches!(preset, AgentPreset::Gemini)
}

fn append_text(answer: &mut String, piece: &str, whole_blocks: bool) {
    if whole_blocks && !answer.is_empty() {
        answer.push_str(BLOCK_SEPARATOR);
    }
    answer.push_str(piece);
}

pub(in crate::environment_shell) fn fold_entry(messages: &mut Vec<ChatMessage>, entry: &ChatEntry) {
    match entry {
        ChatEntry::User {
            text,
            preset,
            model: _,
        } => {
            if let Some(last) = messages.last_mut() {
                last.running = false;
            }
            for (speaker, text, running) in [
                (Speaker::User, text.clone(), false),
                (Speaker::Assistant, String::new(), true),
            ] {
                messages.push(ChatMessage {
                    speaker,
                    text,
                    tools: Vec::new(),
                    parts: Vec::new(),
                    preset: *preset,
                    error: None,
                    running,
                });
            }
        }
        ChatEntry::Event(ChatEvent::Edited { tool, edit }) => place_edit(messages, tool, edit),
        ChatEntry::Asked(outcome) => {
            if let Some(answer) = messages.last_mut().filter(|answer| answer.running) {
                answer.parts.push(AnswerPart::Ask(outcome.clone()));
            }
        }
        ChatEntry::Event(event) => {
            let Some(answer) = messages.last_mut().filter(|answer| answer.running) else {
                return;
            };
            match event {
                ChatEvent::Session(_)
                | ChatEvent::Model(_)
                | ChatEvent::PermissionMode(_)
                | ChatEvent::ContextTokens(_)
                | ChatEvent::ContextWindow(_)
                | ChatEvent::Edited { .. } => {}
                ChatEvent::Text(piece) => {
                    let whole_blocks = joins_whole_blocks(answer.preset);
                    append_text(&mut answer.text, piece, whole_blocks);
                    match answer.parts.last_mut() {
                        Some(AnswerPart::Text(text)) => append_text(text, piece, whole_blocks),
                        _ => answer.parts.push(AnswerPart::Text(piece.clone())),
                    }
                }
                ChatEvent::Thinking(thought) => {
                    answer.parts.push(AnswerPart::Thinking(thought.clone()));
                }
                ChatEvent::Tool(call) => {
                    answer.tools.push(call.clone());
                    answer.parts.push(AnswerPart::Tool(call.clone()));
                }
                ChatEvent::Done => answer.running = false,
                ChatEvent::Error(text) => {
                    answer.running = false;
                    answer.error = Some(text.clone());
                }
            }
        }
    }
}

fn place_edit(messages: &mut [ChatMessage], tool: &ToolUseId, edit: &FileEdit) {
    let answered = |call: &ToolCall| call.id.as_ref() == Some(tool);
    for message in messages.iter_mut().rev() {
        let call = message.parts.iter_mut().rev().find_map(|part| match part {
            AnswerPart::Tool(call) if answered(call) => Some(call),
            _ => None,
        });
        let Some(call) = call else {
            continue;
        };
        call.edit = Some(edit.clone());
        if let Some(listed) = message.tools.iter_mut().rev().find(|call| answered(call)) {
            listed.edit = Some(edit.clone());
        }
        return;
    }
}

pub(in crate::environment_shell) fn last_permission_mode(
    entries: &[ChatEntry],
) -> Option<PermissionMode> {
    entries.iter().rev().find_map(|entry| match entry {
        ChatEntry::Event(ChatEvent::PermissionMode(mode)) => Some(*mode),
        _ => None,
    })
}

pub(in crate::environment_shell) fn replay(
    entries: &[ChatEntry],
) -> (Vec<ChatMessage>, Option<ResumeId>) {
    let mut messages = Vec::new();
    let mut resume = None;
    for entry in entries {
        fold_entry(&mut messages, entry);
        if let ChatEntry::Event(ChatEvent::Session(id)) = entry {
            resume = Some(id.clone());
        }
    }
    if let Some(last) = messages.last_mut() {
        last.running = false;
    }
    (messages, resume)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn record_chat(&mut self, entry: &ChatEntry) {
        fold_entry(&mut self.chat.messages, entry);
        let id = self
            .chat
            .id
            .get_or_insert_with(|| ChatId::from(new_session_id()));
        if self.chat.log.is_none() {
            self.chat.log = ChatLogWriter::spawn();
        }
        if let Some(log) = &self.chat.log {
            log.append(id, entry);
        }
    }

    pub(in crate::environment_shell) fn restore_chat(cx: &mut Context<Self>) {
        let read = cx.background_spawn(async { read_latest_chat() });
        cx.spawn(async move |this, cx| {
            let Some(stored) = read.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell.load_chat(stored);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn load_chat(&mut self, stored: StoredChat) {
        if self.chat.id.is_some() || !self.chat.messages.is_empty() {
            return;
        }
        let (messages, resume) = replay(&stored.entries);
        self.chat.messages = messages;
        self.chat.resume = resume;
        self.chat.permission_mode = last_permission_mode(&stored.entries);
        self.chat.id = Some(stored.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_chat::ToolCall;
    use krad_domain::ModelId;

    fn user(text: &str) -> ChatEntry {
        ChatEntry::User {
            text: text.to_owned(),
            preset: AgentPreset::ClaudeCode,
            model: ModelId::parse("opus"),
        }
    }

    fn event(event: ChatEvent) -> ChatEntry {
        ChatEntry::Event(event)
    }

    #[test]
    fn a_tool_result_patch_replaces_the_diff_of_its_call_after_the_answer() {
        let tool = ToolUseId::from("t1");
        let call = ToolCall {
            id: Some(tool.clone()),
            name: "Edit".to_owned(),
            summary: "src/a.rs".to_owned(),
            edit: None,
        };
        let patch = FileEdit {
            path: "src/a.rs".to_owned(),
            lines: vec![krad_domain::EditLine {
                change: krad_domain::LineChange::Added,
                text: "new".to_owned(),
                old_line: None,
                new_line: Some(12),
            }],
        };
        let mut messages = Vec::new();
        for entry in [
            user("Fix it"),
            event(ChatEvent::Tool(call)),
            event(ChatEvent::Done),
            event(ChatEvent::Edited {
                tool,
                edit: patch.clone(),
            }),
        ] {
            fold_entry(&mut messages, &entry);
        }
        let placed = messages[1].parts.iter().find_map(|part| match part {
            AnswerPart::Tool(call) => call.edit.clone(),
            _ => None,
        });
        assert_eq!(placed, Some(patch.clone()));
        assert_eq!(messages[1].tools[0].edit, Some(patch));
    }

    #[test]
    fn whole_messages_are_separated_and_deltas_are_joined() {
        let mut answer = String::new();
        append_text(&mut answer, "First.", true);
        append_text(&mut answer, "Second.", true);
        assert_eq!(answer, "First.\n\nSecond.");
        let mut streamed = String::new();
        append_text(&mut streamed, "Two ref", false);
        append_text(&mut streamed, "reshes.", false);
        assert_eq!(streamed, "Two refreshes.");
        assert!(!joins_whole_blocks(AgentPreset::Gemini));
        assert!(joins_whole_blocks(AgentPreset::ClaudeCode));
    }

    #[test]
    fn a_stored_transcript_replays_its_answers_and_resume_id() {
        let Ok(id) = "0b7c3a52-9f1e".parse::<ResumeId>() else {
            unreachable!("a valid resume id");
        };
        let read = ToolCall {
            id: None,
            name: "Read".to_owned(),
            summary: "a.rs".to_owned(),
            edit: None,
        };
        let entries = [
            user("Why?"),
            event(ChatEvent::Session(id.clone())),
            event(ChatEvent::Tool(read.clone())),
            event(ChatEvent::Text("Because.".to_owned())),
            event(ChatEvent::Done),
            user("And then?"),
            event(ChatEvent::Error("boom".to_owned())),
            user("Again"),
            event(ChatEvent::Text("Partial".to_owned())),
        ];
        let (messages, resume) = replay(&entries);
        assert_eq!(resume, Some(id));
        let shown: Vec<(Speaker, &str, Option<&str>)> = messages
            .iter()
            .map(|message| {
                (
                    message.speaker,
                    message.text.as_str(),
                    message.error.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                (Speaker::User, "Why?", None),
                (Speaker::Assistant, "Because.", None),
                (Speaker::User, "And then?", None),
                (Speaker::Assistant, "", Some("boom")),
                (Speaker::User, "Again", None),
                (Speaker::Assistant, "Partial", None),
            ]
        );
        assert_eq!(messages[1].tools, std::slice::from_ref(&read));
        assert_eq!(
            messages[1].parts,
            [
                AnswerPart::Tool(read),
                AnswerPart::Text("Because.".to_owned())
            ]
        );
        assert!(messages.iter().all(|message| !message.running));
    }

    #[test]
    fn thinking_tools_and_text_keep_their_order() {
        let grep = ToolCall {
            id: None,
            name: "Grep".to_owned(),
            summary: "refund".to_owned(),
            edit: None,
        };
        let mut messages = Vec::new();
        for entry in [
            user("Where?"),
            event(ChatEvent::Thinking("Search first.".to_owned())),
            event(ChatEvent::Text("Looking.".to_owned())),
            event(ChatEvent::Tool(grep.clone())),
            event(ChatEvent::Text("In refunds.rs.".to_owned())),
            event(ChatEvent::Text("Line 40.".to_owned())),
        ] {
            fold_entry(&mut messages, &entry);
        }
        assert_eq!(
            messages[1].parts,
            [
                AnswerPart::Thinking("Search first.".to_owned()),
                AnswerPart::Text("Looking.".to_owned()),
                AnswerPart::Tool(grep),
                AnswerPart::Text("In refunds.rs.\n\nLine 40.".to_owned()),
            ]
        );
    }

    #[test]
    fn events_after_a_closed_answer_are_ignored() {
        let mut messages = Vec::new();
        for entry in [
            user("hi"),
            event(ChatEvent::Done),
            event(ChatEvent::Text("late".to_owned())),
        ] {
            fold_entry(&mut messages, &entry);
        }
        assert!(messages.iter().all(|message| message.text != "late"));
    }
}
