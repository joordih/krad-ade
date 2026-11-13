use super::*;
use crate::model::ModelId;

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
    let shot = crate::mention::ChatImage {
        path: std::path::PathBuf::from("/shots/1.png"),
        format: crate::mention::ImageFormat::Png,
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
        kind: crate::tool_kind::ToolKind::Shell,
        summary: "printf hi".to_owned(),
        edits: Vec::new(),
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
    assert_eq!(shown.map(|shown| shown.text.as_str()), Some("hi"));
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
        kind: crate::tool_kind::ToolKind::Edit,
        summary: "src/a.rs".to_owned(),
        edits: Vec::new(),
    };
    let patch = FileEdit {
        path: "src/a.rs".to_owned(),
        lines: vec![crate::file_edit::EditLine {
            change: crate::file_edit::LineChange::Added,
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
        AnswerPart::Tool(call) => Some(call.edits.clone()),
        _ => None,
    });
    assert_eq!(placed, Some(vec![patch.clone()]));
    assert_eq!(
        messages[1].tools().next().map(|call| call.edits.clone()),
        Some(vec![patch])
    );
}

#[test]
fn a_rewind_point_lands_on_the_latest_question() {
    let point = crate::rewind::RewindPoint::Prompt(1);
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
    let points: Vec<Option<&crate::rewind::RewindPoint>> = messages
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
fn an_updated_tool_replaces_the_row_with_its_id_in_place() {
    let call = |name: &str| ToolCall {
        id: Some(ToolUseId::from("t1")),
        name: name.to_owned(),
        kind: crate::tool_kind::ToolKind::Read,
        summary: String::new(),
        edits: Vec::new(),
    };
    let mut messages = Vec::new();
    for entry in [
        user("Read it"),
        event(ChatEvent::Tool(call("Read"))),
        event(ChatEvent::Text("Done.".to_owned())),
        event(ChatEvent::ToolUpdated(call("Read src/main.rs"))),
    ] {
        fold_entry(&mut messages, &entry);
    }
    assert_eq!(
        messages[1].parts,
        [
            AnswerPart::Tool(call("Read src/main.rs")),
            AnswerPart::Text("Done.".to_owned()),
        ]
    );
}

#[test]
fn thinking_tools_and_text_keep_their_order() {
    let grep = ToolCall {
        id: None,
        name: "Grep".to_owned(),
        kind: crate::tool_kind::ToolKind::Search,
        summary: "refund".to_owned(),
        edits: Vec::new(),
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
fn a_turn_the_harness_opens_by_itself_continues_the_closed_answer() {
    let mut messages = Vec::new();
    for entry in [
        user("plan it"),
        event(ChatEvent::Text("Waiting for the subagent.".to_owned())),
        event(ChatEvent::Done),
        event(ChatEvent::TurnStarted),
        event(ChatEvent::Text("Here is the plan.".to_owned())),
        event(ChatEvent::Done),
    ] {
        fold_entry(&mut messages, &entry);
    }
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[1].text,
        "Waiting for the subagent.\n\nHere is the plan."
    );
    assert!(!messages[1].running);
}

#[test]
fn a_turn_opened_before_any_answer_starts_one() {
    let mut messages = Vec::new();
    fold_entry(&mut messages, &event(ChatEvent::TurnStarted));
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].speaker, Speaker::Assistant);
    assert!(messages[0].running);
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
