use krad_domain::{AgentPreset, ChatEvent, PermissionMode, ToolCall, ToolUseId};
use serde::Deserialize;
use serde_json::Value;

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
const RUN_TOOL: &str = "Run";
const EDIT_TOOL: &str = "Edit";
const SEARCH_TOOL: &str = "Search";
const UNKNOWN_TOOL: &str = "tool";
const UNKNOWN_ERROR: &str = "the harness reported an error";
const PATH_SEPARATOR: &str = ", ";
const PARENT_TOOL_USE: &str = "/parent_tool_use_id";
const PERMISSION_MODE_KEY: &str = "permissionMode";
const CONTEXT_TOKEN_KEYS: [&str; 3] = [
    "input_tokens",
    "cache_creation_input_tokens",
    "cache_read_input_tokens",
];

#[must_use]
pub fn parse_line(preset: AgentPreset, line: &str) -> Vec<ChatEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    match preset {
        AgentPreset::ClaudeCode => claude(&value),
        AgentPreset::Codex => codex(&value),
        AgentPreset::Opencode => opencode(&value),
        AgentPreset::Gemini => gemini(&value),
        AgentPreset::Shell
        | AgentPreset::Copilot
        | AgentPreset::CursorAgent
        | AgentPreset::Amp
        | AgentPreset::Aider
        | AgentPreset::Goose => Vec::new(),
    }
}

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

fn tool(name: &str, summary: String) -> ChatEvent {
    ChatEvent::Tool(ToolCall {
        id: None,
        name: name.to_owned(),
        summary,
        edit: None,
    })
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
                .map_or_default(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                            Some("text") => block
                                .get("text")
                                .and_then(Value::as_str)
                                .map(|text| ChatEvent::Text(text.to_owned())),
                            Some("thinking") => block
                                .get("thinking")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|thinking| !thinking.is_empty())
                                .map(|thinking| ChatEvent::Thinking(thinking.to_owned())),
                            Some("tool_use") => Some(ChatEvent::Tool(ToolCall {
                                id: block.get("id").and_then(Value::as_str).map(ToolUseId::from),
                                name: block
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or(UNKNOWN_TOOL)
                                    .to_owned(),
                                summary: tool_summary(block.get("input")),
                                edit: block
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .and_then(|name| file_edit(name, block.get("input"))),
                            })),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                });
            blocks.into_iter().chain(context).collect()
        }
        Some("result") => {
            let failed = value.get("is_error").and_then(Value::as_bool) == Some(true)
                || str_at(value, "/subtype").is_some_and(|subtype| subtype != "success");
            if failed {
                vec![error(
                    str_at(value, "/result").or(str_at(value, "/subtype")),
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

fn codex(value: &Value) -> Vec<ChatEvent> {
    match str_at(value, "/type") {
        Some("item.completed") => {
            let item = value.get("item");
            let text_of = |pointer: &str| item.and_then(|item| str_at(item, pointer));
            match text_of("/type") {
                Some("agent_message") => {
                    text_of("/text").map_or_default(|text| vec![ChatEvent::Text(text.to_owned())])
                }
                Some("command_execution") => {
                    vec![tool(
                        RUN_TOOL,
                        text_of("/command").unwrap_or_default().to_owned(),
                    )]
                }
                Some("file_change") => {
                    let paths: Vec<&str> = item
                        .and_then(|item| item.get("changes"))
                        .and_then(Value::as_array)
                        .map_or_default(|changes| {
                            changes
                                .iter()
                                .filter_map(|change| str_at(change, "/path"))
                                .collect()
                        });
                    vec![tool(EDIT_TOOL, paths.join(PATH_SEPARATOR))]
                }
                Some("web_search") => {
                    vec![tool(
                        SEARCH_TOOL,
                        text_of("/query").unwrap_or_default().to_owned(),
                    )]
                }
                Some("mcp_tool_call") => vec![tool(
                    text_of("/tool").unwrap_or(UNKNOWN_TOOL),
                    String::new(),
                )],
                _ => Vec::new(),
            }
        }
        Some("turn.completed") => vec![ChatEvent::Done],
        Some("turn.failed") => vec![error(str_at(value, "/error/message"))],
        Some("error") => vec![error(str_at(value, "/message"))],
        _ => Vec::new(),
    }
}

fn opencode(value: &Value) -> Vec<ChatEvent> {
    match str_at(value, "/type") {
        Some("text") => str_at(value, "/part/text")
            .map_or_default(|text| vec![ChatEvent::Text(text.to_owned())]),
        Some("tool_use" | "tool") => vec![tool(
            str_at(value, "/part/tool").unwrap_or(UNKNOWN_TOOL),
            tool_summary(value.pointer("/part/state/input")),
        )],
        Some("error") => vec![error(
            str_at(value, "/error/data/message")
                .or(str_at(value, "/error/message"))
                .or(str_at(value, "/message")),
        )],
        _ => Vec::new(),
    }
}

fn gemini(value: &Value) -> Vec<ChatEvent> {
    match str_at(value, "/type") {
        Some("message") if str_at(value, "/role") == Some("assistant") => {
            str_at(value, "/content").map_or_default(|text| vec![ChatEvent::Text(text.to_owned())])
        }
        Some("tool_use") => vec![tool(
            str_at(value, "/tool_name").unwrap_or(UNKNOWN_TOOL),
            tool_summary(value.get("parameters")),
        )],
        Some("result") => {
            if str_at(value, "/status") == Some("success") {
                vec![ChatEvent::Done]
            } else {
                vec![error(str_at(value, "/error/message"))]
            }
        }
        Some("error") => vec![error(str_at(value, "/message"))],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_that_are_not_json_are_skipped() {
        assert_eq!(
            parse_line(AgentPreset::ClaudeCode, "warning: update available"),
            []
        );
        assert_eq!(parse_line(AgentPreset::Codex, ""), []);
        assert_eq!(parse_line(AgentPreset::Shell, r#"{"type":"text"}"#), []);
    }

    #[test]
    fn claude_failures_carry_their_message() {
        let events = parse_line(
            AgentPreset::ClaudeCode,
            r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#,
        );
        assert_eq!(events, [ChatEvent::Error("error_max_turns".to_owned())]);
    }

    #[test]
    fn claude_reports_its_permission_mode_when_it_starts_and_when_it_changes() {
        let started = parse_line(
            AgentPreset::ClaudeCode,
            r#"{"type":"system","subtype":"init","permissionMode":"default"}"#,
        );
        let changed = parse_line(
            AgentPreset::ClaudeCode,
            r#"{"type":"system","subtype":"status","status":null,"permissionMode":"plan"}"#,
        );
        assert_eq!(started, [ChatEvent::PermissionMode(PermissionMode::Manual)]);
        assert_eq!(changed, [ChatEvent::PermissionMode(PermissionMode::Plan)]);
    }

    #[test]
    fn a_permission_mode_claude_code_added_later_is_skipped() {
        let events = parse_line(
            AgentPreset::ClaudeCode,
            r#"{"type":"system","subtype":"status","permissionMode":"yolo"}"#,
        );
        assert_eq!(events, []);
    }
}
