use krad_chat::{ChatEvent, Speaker};
use krad_domain::{
    AgentPreset, ChatEntry, FileEdit, Mention, PermissionMode, Prompt, ToolCall, ToolOutput,
    ToolUseId,
};
use std::collections::HashMap;

use super::{AnswerPart, ChatMessage, ShownOutput};

const BLOCK_SEPARATOR: &str = "\n\n";

fn append_text(answer: &mut String, piece: &str, whole_blocks: bool) {
    if whole_blocks && !answer.is_empty() {
        answer.push_str(BLOCK_SEPARATOR);
    }
    answer.push_str(piece);
}

pub(in crate::environment_shell) fn fold_prompt(
    messages: &mut Vec<ChatMessage>,
    prompt: &Prompt,
    preset: AgentPreset,
) {
    fold_entry(
        messages,
        &ChatEntry::User {
            text: prompt.text.clone(),
            preset,
            model: None,
            images: Vec::new(),
        },
    );
    if let Some(asked) = messages
        .iter_mut()
        .rev()
        .find(|message| message.speaker == Speaker::User)
    {
        asked.mentions.clone_from(&prompt.mentions);
    }
}

pub(in crate::environment_shell) fn fold_entry(messages: &mut Vec<ChatMessage>, entry: &ChatEntry) {
    match entry {
        ChatEntry::User {
            text,
            preset: _,
            model: _,
            images,
        } => {
            if let Some(last) = messages.last_mut() {
                last.running = false;
            }
            let shown: Vec<Mention> = images.iter().cloned().map(Mention::Image).collect();
            for (speaker, text, running, mentions) in [
                (Speaker::User, text.clone(), false, shown),
                (Speaker::Assistant, String::new(), true, Vec::new()),
            ] {
                messages.push(ChatMessage {
                    speaker,
                    text,
                    parts: Vec::new(),
                    error: None,
                    running,
                    mentions,
                    checklist: None,
                    rewind: None,
                    tool_outputs: HashMap::new(),
                });
            }
        }
        ChatEntry::Event(ChatEvent::Edited { tool, edit }) => place_edit(messages, tool, edit),
        ChatEntry::Event(ChatEvent::ToolOutput { tool, output }) => {
            place_output(messages, tool, output);
        }
        ChatEntry::Event(ChatEvent::Prompted(point)) => {
            if let Some(asked) = messages
                .iter_mut()
                .rev()
                .find(|message| message.speaker == Speaker::User)
            {
                asked.rewind = Some(point.clone());
            }
        }
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
                | ChatEvent::Prompted(_)
                | ChatEvent::Edited { .. }
                | ChatEvent::ToolOutput { .. } => {}
                ChatEvent::Checklist(list) => answer.checklist = Some(list.clone()),
                ChatEvent::Text(piece) | ChatEvent::TextDelta(piece) => {
                    let whole_blocks = matches!(event, ChatEvent::Text(_));
                    append_text(&mut answer.text, piece, whole_blocks);
                    match answer.parts.last_mut() {
                        Some(AnswerPart::Text(text)) => append_text(text, piece, whole_blocks),
                        _ => answer.parts.push(AnswerPart::Text(piece.clone())),
                    }
                }
                ChatEvent::Thinking(thought) => {
                    answer.parts.push(AnswerPart::Thinking(thought.clone()));
                }
                ChatEvent::Tool(call) => answer.parts.push(AnswerPart::Tool(call.clone())),
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
        return;
    }
}

fn place_output(messages: &mut [ChatMessage], tool: &ToolUseId, output: &ToolOutput) {
    let ran = |part: &AnswerPart| matches!(part, AnswerPart::Tool(call) if call.id.as_ref() == Some(tool));
    if let Some(message) = messages
        .iter_mut()
        .rev()
        .find(|message| message.parts.iter().any(ran))
    {
        message
            .tool_outputs
            .insert(tool.clone(), ShownOutput::from(output));
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
            images: Vec::new(),
        }
    }

    fn event(event: ChatEvent) -> ChatEntry {
        ChatEntry::Event(event)
    }

    #[test]
    fn images_saved_with_a_user_entry_come_back_as_its_mentions() {
        let shot = krad_domain::ChatImage {
            path: std::path::PathBuf::from("/shots/1.png"),
            format: krad_domain::ImageFormat::Png,
        };
        let mut messages = Vec::new();
        fold_entry(
            &mut messages,
            &ChatEntry::User {
                text: "Look".to_owned(),
                preset: AgentPreset::Gateway,
                model: None,
                images: vec![shot.clone()],
            },
        );
        assert_eq!(messages[0].mentions, [Mention::Image(shot)]);
        assert_eq!(messages[1].mentions, []);
    }

    #[test]
    fn a_terminal_output_lands_on_its_tool_and_the_latest_one_wins() {
        let tool = ToolUseId::from("t1");
        let call = ToolCall {
            id: Some(tool.clone()),
            name: "printf".to_owned(),
            kind: krad_domain::ToolKind::Shell,
            summary: "printf hi".to_owned(),
            edit: None,
        };
        let output = |text: &str, exit_code| {
            event(ChatEvent::ToolOutput {
                tool: tool.clone(),
                output: ToolOutput {
                    text: text.to_owned(),
                    exit_code,
                    ..ToolOutput::default()
                },
            })
        };
        let mut messages = Vec::new();
        for entry in [
            user("Run it"),
            event(ChatEvent::Tool(call)),
            output("h", None),
            output("hi\n", Some(3)),
            event(ChatEvent::ToolOutput {
                tool: ToolUseId::from("unknown"),
                output: ToolOutput::default(),
            }),
            event(ChatEvent::Done),
        ] {
            fold_entry(&mut messages, &entry);
        }
        let shown = messages[1].tool_outputs.get(&tool);
        assert_eq!(shown.map(|shown| shown.text.as_ref()), Some("hi"));
        assert_eq!(
            shown.and_then(|shown| shown.failure.as_deref()),
            Some("Exited with code 3")
        );
        assert_eq!(messages[1].tool_outputs.len(), 1);
    }

    #[test]
    fn a_tool_result_patch_replaces_the_diff_of_its_call_after_the_answer() {
        let tool = ToolUseId::from("t1");
        let call = ToolCall {
            id: Some(tool.clone()),
            name: "Edit".to_owned(),
            kind: krad_domain::ToolKind::Edit,
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
        assert_eq!(
            messages[1]
                .tools()
                .next()
                .and_then(|call| call.edit.clone()),
            Some(patch)
        );
    }

    #[test]
    fn a_rewind_point_lands_on_the_latest_question() {
        let point = krad_domain::RewindPoint::Prompt(1);
        let mut messages = Vec::new();
        for entry in [
            user("One"),
            event(ChatEvent::Done),
            user("Two"),
            event(ChatEvent::Text("Sure.".to_owned())),
            event(ChatEvent::Prompted(point.clone())),
        ] {
            fold_entry(&mut messages, &entry);
        }
        let points: Vec<Option<&krad_domain::RewindPoint>> = messages
            .iter()
            .map(|message| message.rewind.as_ref())
            .collect();
        assert_eq!(points, [None, None, Some(&point), None]);
    }

    #[test]
    fn whole_messages_are_separated_and_deltas_are_joined() {
        let answer = |pieces: [ChatEvent; 2]| {
            let mut messages = Vec::new();
            fold_entry(&mut messages, &user("Go"));
            for piece in pieces {
                fold_entry(&mut messages, &event(piece));
            }
            messages[1].text.clone()
        };
        assert_eq!(
            answer([
                ChatEvent::Text("First.".to_owned()),
                ChatEvent::Text("Second.".to_owned())
            ]),
            "First.\n\nSecond."
        );
        assert_eq!(
            answer([
                ChatEvent::TextDelta("Two ref".to_owned()),
                ChatEvent::TextDelta("reshes.".to_owned())
            ]),
            "Two refreshes."
        );
    }

    #[test]
    fn thinking_tools_and_text_keep_their_order() {
        let grep = ToolCall {
            id: None,
            name: "Grep".to_owned(),
            kind: krad_domain::ToolKind::Search,
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
    fn an_error_ends_the_answer_and_keeps_its_text() {
        let mut messages = Vec::new();
        for entry in [
            user("And then?"),
            event(ChatEvent::Error("boom".to_owned())),
        ] {
            fold_entry(&mut messages, &entry);
        }
        assert_eq!(messages[1].error.as_deref(), Some("boom"));
        assert!(!messages[1].running);
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
