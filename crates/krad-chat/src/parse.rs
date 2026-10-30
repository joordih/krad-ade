use krad_domain::{ChatEvent, PermissionMode, ToolCall, ToolUseId};
use serde::Deserialize;
use serde_json::Value;

use crate::checklist::{todos, TODO_TOOL};
use crate::edits::{file_edit, patched_edit};

const SUMMARY_KEYS: &[&str] = &[
    "file_path",
    "filePath",
    "path",
    "command",
    "pattern",
    "query",
    "url",
    "description",
];
const UNKNOWN_TOOL: &str = "tool";
const UNKNOWN_ERROR: &str = "the harness reported an error";
const PARENT_TOOL_USE: &str = "/parent_tool_use_id";
const FIRST_ERROR: &str = "/errors/0";
const PERMISSION_MODE_KEY: &str = "permissionMode";
const CONTEXT_TOKEN_KEYS: [&str; 3] = [
    "input_tokens",
    "cache_creation_input_tokens",
    "cache_read_input_tokens",
];

fn str_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

pub(crate) fn tool_summary(input: Option<&Value>) -> String {
    input
        .and_then(|input| {
            SUMMARY_KEYS
                .iter()
                .find_map(|key| input.get(key).and_then(Value::as_str))
        })
        .unwrap_or_default()
        .to_owned()
}

fn error(message: Option<&str>) -> ChatEvent {
    ChatEvent::Error(message.unwrap_or(UNKNOWN_ERROR).to_owned())
}

pub(crate) fn claude(value: &Value) -> Vec<ChatEvent> {
    if str_at(value, PARENT_TOOL_USE).is_some() {
        return Vec::new();
    }
    match str_at(value, "/type") {
        Some("system") if str_at(value, "/subtype") == Some("init") => {
            let session = str_at(value, "/session_id")
                .and_then(|id| id.parse().ok())
                .map(ChatEvent::Session);
            let model = str_at(value, "/model").map(|model| ChatEvent::Model(model.to_owned()));
            session
                .into_iter()
                .chain(model)
                .chain(permission_mode(value))
                .collect()
        }
        Some("system") if str_at(value, "/subtype") == Some("status") => {
            permission_mode(value).into_iter().collect()
        }
        Some("assistant") => {
            let context = context_tokens(value).map(ChatEvent::ContextTokens);
            let blocks = value
                .pointer("/message/content")
                .and_then(Value::as_array)
                .map_or_default(|blocks| blocks.iter().flat_map(block_events).collect::<Vec<_>>());
            blocks.into_iter().chain(context).collect()
        }
        Some("result") => {
            let failed = value.get("is_error").and_then(Value::as_bool) == Some(true)
                || str_at(value, "/subtype").is_some_and(|subtype| subtype != "success");
            if failed {
                vec![error(
                    str_at(value, "/result")
                        .or(str_at(value, FIRST_ERROR))
                        .or(str_at(value, "/subtype")),
                )]
            } else {
                context_window(value)
                    .map(ChatEvent::ContextWindow)
                    .into_iter()
                    .chain([ChatEvent::Done])
                    .collect()
            }
        }
        Some("user") => patched_edit(value).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn block_events(block: &Value) -> Vec<ChatEvent> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => block
            .get("text")
            .and_then(Value::as_str)
            .map(|text| ChatEvent::Text(text.to_owned()))
            .into_iter()
            .collect(),
        Some("thinking") => block
            .get("thinking")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|thinking| !thinking.is_empty())
            .map(|thinking| ChatEvent::Thinking(thinking.to_owned()))
            .into_iter()
            .collect(),
        Some("tool_use") => tool_use_events(block),
        _ => Vec::new(),
    }
}

fn tool_use_events(block: &Value) -> Vec<ChatEvent> {
    let name = block.get("name").and_then(Value::as_str);
    let input = block.get("input");
    let tool = ChatEvent::Tool(ToolCall {
        id: block.get("id").and_then(Value::as_str).map(ToolUseId::from),
        name: name.unwrap_or(UNKNOWN_TOOL).to_owned(),
        summary: tool_summary(input),
        edit: name.and_then(|name| file_edit(name, input)),
    });
    let checklist = (name == Some(TODO_TOOL))
        .then(|| input.and_then(todos))
        .flatten()
        .map(ChatEvent::Checklist);
    std::iter::once(tool).chain(checklist).collect()
}

pub(crate) fn permission_mode(value: &Value) -> Option<ChatEvent> {
    let mode = value.get(PERMISSION_MODE_KEY)?;
    PermissionMode::deserialize(mode)
        .ok()
        .map(ChatEvent::PermissionMode)
}

fn context_tokens(value: &Value) -> Option<u64> {
    let usage = value.pointer("/message/usage")?;
    let total: u64 = CONTEXT_TOKEN_KEYS
        .iter()
        .filter_map(|key| usage.get(key).and_then(Value::as_u64))
        .sum();
    (total > 0).then_some(total)
}

fn context_window(value: &Value) -> Option<u64> {
    value
        .get("modelUsage")?
        .as_object()?
        .values()
        .filter_map(|usage| usage.get("contextWindow").and_then(Value::as_u64))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ResumeId;

    fn events(transcript: &str) -> Vec<ChatEvent> {
        transcript
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .flat_map(|value| claude(&value))
            .collect()
    }

    fn model(name: &str) -> ChatEvent {
        ChatEvent::Model(name.to_owned())
    }

    fn session(id: &str) -> ChatEvent {
        let Ok(id) = id.parse::<ResumeId>() else {
            unreachable!("{id} is a valid resume id");
        };
        ChatEvent::Session(id)
    }

    fn claude_tool(id: &str, name: &str, summary: &str) -> ChatEvent {
        ChatEvent::Tool(ToolCall {
            id: Some(ToolUseId::from(id)),
            name: name.to_owned(),
            summary: summary.to_owned(),
            edit: None,
        })
    }

    #[test]
    fn claude_failures_carry_their_message() {
        let failed = events(r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#);
        assert_eq!(failed, [ChatEvent::Error("error_max_turns".to_owned())]);
        let refused = events(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["Resume rejected by --resume-drops-turn: range"]}"#,
        );
        assert_eq!(
            refused,
            [ChatEvent::Error(
                "Resume rejected by --resume-drops-turn: range".to_owned()
            )]
        );
    }

    #[test]
    fn claude_reports_its_permission_mode_when_it_starts_and_when_it_changes() {
        let started = events(r#"{"type":"system","subtype":"init","permissionMode":"default"}"#);
        let changed =
            events(r#"{"type":"system","subtype":"status","status":null,"permissionMode":"plan"}"#);
        assert_eq!(started, [ChatEvent::PermissionMode(PermissionMode::Manual)]);
        assert_eq!(changed, [ChatEvent::PermissionMode(PermissionMode::Plan)]);
    }

    #[test]
    fn a_permission_mode_claude_code_added_later_is_skipped() {
        let events = events(r#"{"type":"system","subtype":"status","permissionMode":"yolo"}"#);
        assert_eq!(events, []);
    }

    #[test]
    fn claude_transcript_yields_the_read_the_answer_and_done() {
        assert_eq!(
            events(include_str!("../tests/fixtures/claude.jsonl")),
            [
                session("s1"),
                model("claude-opus"),
                claude_tool("t1", "Read", "server/auth/refresh.ts"),
                ChatEvent::Text("Both tabs call `rotate()` at once.".to_owned()),
                ChatEvent::Done,
            ]
        );
    }

    #[test]
    fn a_recorded_claude_turn_parses_to_its_answer() {
        assert_eq!(
            events(include_str!("../tests/fixtures/claude-recorded.jsonl")),
            [
                ChatEvent::Text("pong".to_owned()),
                ChatEvent::ContextTokens(33_694),
                ChatEvent::ContextWindow(1_000_000),
                ChatEvent::Done
            ],
            "the context the answer took, and the window it fits in"
        );
    }

    #[test]
    fn a_live_claude_conversation_reports_its_session_and_closes_each_answer() {
        let id = "0b7c3a52-9f1e-4d2a-8c6b-1f2e3d4c5b6a";
        assert_eq!(
            events(include_str!("../tests/fixtures/claude-live.jsonl")),
            [
                session(id),
                model("claude-opus"),
                ChatEvent::Text("Two tabs rotate at once.".to_owned()),
                ChatEvent::Done,
                session(id),
                model("claude-opus"),
                claude_tool("t1", "Edit", "server/auth/refresh.ts"),
                ChatEvent::Text("Shared one in-flight promise.".to_owned()),
                ChatEvent::Done,
            ]
        );
    }

    #[test]
    fn a_todo_write_keeps_its_tool_row_and_sends_the_checklist() {
        let events = events(concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t4","name":"TodoWrite","input":{"todos":["#,
            r#"{"content":"Fix the bug","status":"in_progress","activeForm":"Fixing the bug"},"#,
            r#"{"content":"Run the tests","status":"pending","activeForm":"Running the tests"}]}}]}}"#
        ));
        assert_eq!(events[0], claude_tool("t4", "TodoWrite", ""));
        let ChatEvent::Checklist(list) = &events[1] else {
            unreachable!("a TodoWrite carries a checklist: {events:?}");
        };
        assert_eq!(list.items.len(), 2);
        assert_eq!(
            list.current().map(|item| item.text.as_str()),
            Some("Fix the bug")
        );
    }
}
