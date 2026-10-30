use crate::client::call;
use crate::env::{ENV_ENDPOINT, ENV_TERMINAL, ENV_TOKEN};
use crate::handle::Caller;
use crate::protocol::{BusToken, Request, Response};
use krad_domain::{AgentPreset, ToolDecision, CLAUDE_PERMISSION_REQUEST, CLAUDE_PRE_TOOL_USE};
use serde_json::{json, Value};
use std::io::{Read as _, Write as _};

const MAX_PAYLOAD_BYTES: u64 = 1024 * 1024;
const DETAIL_CHARS: usize = 120;
const EVENT_FLAG: &str = "--event";
const CLAUDE_EVENT_KEY: &str = "hook_event_name";
const CLAUDE_TOOL_KEY: &str = "tool_name";
const CLAUDE_NOTIFICATION_KEY: &str = "notification_type";
const CLAUDE_SOURCE_KEY: &str = "source";
const CLAUDE_SESSION_KEY: &str = "session_id";
const CLAUDE_INPUT_KEY: &str = "tool_input";
const CLAUDE_MESSAGE_KEY: &str = "message";
const CLAUDE_SUGGESTIONS_KEY: &str = "permission_suggestions";
const CLAUDE_ALLOW: &str = "allow";
const CLAUDE_DENY: &str = "deny";
const INPUT_PREVIEW_KEYS: [&str; 5] = ["command", "file_path", "pattern", "url", "description"];
const CODEX_TYPE_KEY: &str = "type";
const CODEX_MESSAGE_KEY: &str = "last-assistant-message";
const CODEX_TITLE_KEY: &str = "title";
const CODEX_THREAD_KEY: &str = "thread-id";
const EXIT_OK: i32 = 0;

#[must_use]
pub fn run_hook(harness: AgentPreset, args: &[String]) -> i32 {
    let request = match harness {
        AgentPreset::ClaudeCode => read_stdin().and_then(|payload| claude_request(&payload)),
        AgentPreset::Codex => args.last().and_then(|payload| codex_request(payload)),
        AgentPreset::Opencode => opencode_request(args),
        AgentPreset::Shell | AgentPreset::Gateway => None,
    };
    if let Some(request) = request {
        send(request);
    }
    EXIT_OK
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AskHook {
    PreToolUse,
    PermissionRequest,
}

impl AskHook {
    fn named(event: &str) -> Option<Self> {
        match event {
            CLAUDE_PRE_TOOL_USE => Some(Self::PreToolUse),
            CLAUDE_PERMISSION_REQUEST => Some(Self::PermissionRequest),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::PreToolUse => CLAUDE_PRE_TOOL_USE,
            Self::PermissionRequest => CLAUDE_PERMISSION_REQUEST,
        }
    }
}

#[must_use]
pub fn run_ask(harness: AgentPreset) -> i32 {
    if harness != AgentPreset::ClaudeCode {
        return EXIT_OK;
    }
    let Some((hook, request)) = read_stdin().and_then(|payload| claude_ask(&payload)) else {
        return EXIT_OK;
    };
    if let Some(Response::Decided(decision)) = ask_app(request) {
        if let Some(output) = claude_decision(hook, &decision) {
            std::io::stdout()
                .write_all(output.to_string().as_bytes())
                .unwrap_or_default();
        }
    }
    EXIT_OK
}

fn claude_ask(payload: &str) -> Option<(AskHook, Request)> {
    let value: Value = serde_json::from_str(payload).ok()?;
    let hook = AskHook::named(value.get(CLAUDE_EVENT_KEY)?.as_str()?)?;
    let request = Request::Ask {
        tool: value.get(CLAUDE_TOOL_KEY)?.as_str()?.to_owned(),
        input: value.get(CLAUDE_INPUT_KEY).cloned().unwrap_or_default(),
        suggestions: value
            .get(CLAUDE_SUGGESTIONS_KEY)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    Some((hook, request))
}

fn claude_decision(hook: AskHook, decision: &ToolDecision) -> Option<Value> {
    let specific = match (hook, decision) {
        (_, ToolDecision::Pass) => return None,
        (
            AskHook::PreToolUse,
            ToolDecision::Allow {
                updated_input,
                permissions: _,
            },
        ) => {
            let mut output = json!({ "permissionDecision": CLAUDE_ALLOW });
            if let Some(input) = updated_input {
                output["updatedInput"] = input.clone();
            }
            output
        }
        (AskHook::PreToolUse, ToolDecision::Deny { message, .. }) => json!({
            "permissionDecision": CLAUDE_DENY,
            "permissionDecisionReason": message,
        }),
        (
            AskHook::PermissionRequest,
            ToolDecision::Allow {
                updated_input,
                permissions,
            },
        ) => {
            let mut verdict = json!({ "behavior": CLAUDE_ALLOW });
            if let Some(input) = updated_input {
                verdict["updatedInput"] = input.clone();
            }
            if !permissions.is_empty() {
                verdict["updatedPermissions"] = Value::from(permissions.clone());
            }
            json!({ "decision": verdict })
        }
        (AskHook::PermissionRequest, ToolDecision::Deny { message, interrupt }) => json!({
            "decision": {
                "behavior": CLAUDE_DENY,
                "message": message,
                "interrupt": interrupt,
            },
        }),
    };
    let mut specific = specific;
    specific["hookEventName"] = Value::from(hook.name());
    Some(json!({ "hookSpecificOutput": specific }))
}

pub(crate) fn read_stdin() -> Option<String> {
    let mut payload = String::new();
    std::io::stdin()
        .take(MAX_PAYLOAD_BYTES)
        .read_to_string(&mut payload)
        .ok()?;
    Some(payload)
}

fn preview(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    line.chars().take(DETAIL_CHARS).collect()
}

fn claude_request(payload: &str) -> Option<Request> {
    let value: Value = serde_json::from_str(payload).ok()?;
    let event = value.get(CLAUDE_EVENT_KEY)?.as_str()?.to_owned();
    let tool = [CLAUDE_TOOL_KEY, CLAUDE_NOTIFICATION_KEY, CLAUDE_SOURCE_KEY]
        .iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(str::to_owned);
    let input = value.get(CLAUDE_INPUT_KEY);
    let detail = INPUT_PREVIEW_KEYS
        .iter()
        .find_map(|key| {
            input
                .and_then(|input| input.get(key))
                .and_then(Value::as_str)
        })
        .or_else(|| value.get(CLAUDE_MESSAGE_KEY).and_then(Value::as_str))
        .map(preview);
    let resume = value
        .get(CLAUDE_SESSION_KEY)
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some(Request::Hook {
        harness: AgentPreset::ClaudeCode,
        event,
        tool,
        detail,
        resume,
    })
}

fn codex_request(payload: &str) -> Option<Request> {
    let value: Value = serde_json::from_str(payload).ok()?;
    let message = value.get(CODEX_MESSAGE_KEY).and_then(Value::as_str);
    if message.is_some_and(is_codex_title) {
        return None;
    }
    Some(Request::Hook {
        harness: AgentPreset::Codex,
        event: value.get(CODEX_TYPE_KEY)?.as_str()?.to_owned(),
        tool: None,
        detail: message.map(preview),
        resume: value
            .get(CODEX_THREAD_KEY)
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn is_codex_title(message: &str) -> bool {
    serde_json::from_str::<Value>(message).is_ok_and(|reply| {
        reply
            .as_object()
            .is_some_and(|reply| reply.contains_key(CODEX_TITLE_KEY))
    })
}

fn opencode_request(args: &[String]) -> Option<Request> {
    let position = args.iter().position(|arg| arg == EVENT_FLAG)?;
    Some(Request::Hook {
        harness: AgentPreset::Opencode,
        event: args.get(position + 1)?.clone(),
        tool: None,
        detail: None,
        resume: None,
    })
}

pub(crate) fn send(request: Request) {
    ask_app(request).map_or_default(drop);
}

fn ask_app(request: Request) -> Option<Response> {
    let (Ok(endpoint), Ok(token), Ok(terminal)) = (
        std::env::var(ENV_ENDPOINT),
        std::env::var(ENV_TOKEN),
        std::env::var(ENV_TERMINAL),
    ) else {
        return None;
    };
    let from = terminal.parse::<Caller>().ok()?;
    call(&endpoint, &BusToken::from(token), &from, request).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answered_questions_go_back_as_the_updated_input_before_the_tool_runs() {
        let payload = r#"{"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion",
            "tool_input":{"questions":[{"question":"Pick a color"}]}}"#;
        let Some((hook, Request::Ask { tool, input, .. })) = claude_ask(payload) else {
            unreachable!("an Ask the hook holds");
        };
        assert_eq!(tool, "AskUserQuestion");
        let mut answered = input;
        answered["answers"] = json!({ "Pick a color": "Blue" });
        let decision = ToolDecision::Allow {
            updated_input: Some(answered.clone()),
            permissions: Vec::new(),
        };
        assert_eq!(
            claude_decision(hook, &decision),
            Some(json!({ "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": answered,
            }}))
        );
        assert_eq!(claude_decision(hook, &ToolDecision::Pass), None);
    }

    #[test]
    fn a_permission_is_granted_with_its_suggestions_or_denied_stopping_the_answer() {
        let rule = json!({ "type": "setMode", "mode": "acceptEdits", "destination": "session" });
        let payload = json!({
            "hook_event_name": "PermissionRequest",
            "tool_name": "Write",
            "tool_input": { "file_path": "a.txt" },
            "permission_suggestions": [rule],
        })
        .to_string();
        let Some((hook, Request::Ask { suggestions, .. })) = claude_ask(&payload) else {
            unreachable!("an Ask the hook holds");
        };
        let granted = ToolDecision::Allow {
            updated_input: None,
            permissions: suggestions,
        };
        assert_eq!(
            claude_decision(hook, &granted),
            Some(json!({ "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": { "behavior": "allow", "updatedPermissions": [rule] },
            }}))
        );
        let denied = ToolDecision::Deny {
            message: "Stop here.".to_owned(),
            interrupt: true,
        };
        assert_eq!(
            claude_decision(hook, &denied),
            Some(json!({ "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": { "behavior": "deny", "message": "Stop here.", "interrupt": true },
            }}))
        );
        assert_eq!(claude_ask(r#"{"hook_event_name":"Stop"}"#), None);
    }

    #[test]
    fn claude_payloads_become_hook_requests_with_a_preview() {
        let payload = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash",
            "tool_input":{"command":"cargo test --workspace\necho done"}}"#;
        assert_eq!(
            claude_request(payload),
            Some(Request::Hook {
                harness: AgentPreset::ClaudeCode,
                event: "PreToolUse".to_owned(),
                tool: Some("Bash".to_owned()),
                detail: Some("cargo test --workspace".to_owned()),
                resume: None,
            })
        );
        assert_eq!(claude_request("not json"), None);
        let idle = r#"{"hook_event_name":"Notification","notification_type":"idle_prompt",
            "message":"Claude is waiting for your input"}"#;
        assert!(matches!(
            claude_request(idle),
            Some(Request::Hook { tool: Some(kind), .. }) if kind == "idle_prompt"
        ));
        let start = r#"{"hook_event_name":"SessionStart","source":"resume",
            "session_id":"0b7c3a52-9f1e"}"#;
        assert!(matches!(
            claude_request(start),
            Some(Request::Hook { tool: Some(source), resume: Some(id), .. })
                if source == "resume" && id == "0b7c3a52-9f1e"
        ));
        let codex = r#"{"type":"agent-turn-complete","thread-id":"01a18999-1b58","last-assistant-message":"All tests pass."}"#;
        assert!(matches!(
            codex_request(codex),
            Some(Request::Hook { event, detail: Some(detail), resume: Some(thread), .. })
                if event == "agent-turn-complete" && detail == "All tests pass." && thread == "01a18999-1b58"
        ));
        let title =
            r#"{"type":"agent-turn-complete","last-assistant-message":"{\"title\":\"Run echo\"}"}"#;
        assert_eq!(codex_request(title), None);
        let args = ["--event".to_owned(), "session.idle".to_owned()];
        assert!(matches!(
            opencode_request(&args),
            Some(Request::Hook { event, .. }) if event == "session.idle"
        ));
    }
}
