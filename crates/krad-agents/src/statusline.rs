use crate::hook::{read_stdin, send};
use crate::protocol::{AgentStatus, Request};
use serde_json::Value;
use std::io::Write as _;

const EXIT_OK: i32 = 0;
const MODEL_ID: &str = "/model/id";
const MODEL_NAME: &str = "/model/display_name";
const CONTEXT_USED: &str = "/context_window/used_percentage";
const EFFORT: &str = "/effort/level";
const THINKING: &str = "/thinking/enabled";
const FIVE_HOUR_USED: &str = "/rate_limits/five_hour/used_percentage";
const SEVEN_DAY_USED: &str = "/rate_limits/seven_day/used_percentage";
const USER_COMMAND: &str = "/statusLine/command";

#[must_use]
pub fn run_status_line(args: &[String]) -> i32 {
    if let Some(status) = read_stdin().and_then(|payload| claude_status(&payload)) {
        send(Request::Status(status));
    }
    if let Some(command) = args.get(1).and_then(|settings| user_command(settings)) {
        std::io::stdout()
            .write_all(command.as_bytes())
            .unwrap_or_default();
    }
    EXIT_OK
}

fn text_at(value: &Value, pointer: &str) -> Option<String> {
    value.pointer(pointer)?.as_str().map(str::to_owned)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn percent_at(value: &Value, pointer: &str) -> Option<u16> {
    let raw = value.pointer(pointer)?.as_f64()?;
    Some(raw.round().clamp(0.0, f64::from(u16::MAX)) as u16)
}

fn claude_status(payload: &str) -> Option<AgentStatus> {
    let value: Value = serde_json::from_str(payload).ok()?;
    Some(AgentStatus {
        model_id: text_at(&value, MODEL_ID),
        model_name: text_at(&value, MODEL_NAME),
        context_used: percent_at(&value, CONTEXT_USED),
        effort: text_at(&value, EFFORT),
        thinking: value.pointer(THINKING).and_then(Value::as_bool),
        five_hour_used: percent_at(&value, FIVE_HOUR_USED),
        seven_day_used: percent_at(&value, SEVEN_DAY_USED),
    })
}

fn user_command(settings: &str) -> Option<String> {
    let value: Value = serde_json::from_str(settings).ok()?;
    text_at(&value, USER_COMMAND)
        .map(|command| command.trim().to_owned())
        .filter(|command| !command.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_payload_yields_model_context_and_usage() {
        let payload = r#"{"model":{"id":"claude-opus-5-5","display_name":"Opus 5.5"},
            "context_window":{"used_percentage":8.4},"effort":{"level":"high"},
            "thinking":{"enabled":true},
            "rate_limits":{"five_hour":{"used_percentage":23.5},"seven_day":{"used_percentage":41.2}}}"#;
        assert_eq!(
            claude_status(payload),
            Some(AgentStatus {
                model_id: Some("claude-opus-5-5".to_owned()),
                model_name: Some("Opus 5.5".to_owned()),
                context_used: Some(8),
                effort: Some("high".to_owned()),
                thinking: Some(true),
                five_hour_used: Some(24),
                seven_day_used: Some(41),
            })
        );
    }

    #[test]
    fn missing_fields_stay_unknown() {
        let status = claude_status(r#"{"model":{"display_name":"Sonnet"}}"#);
        assert_eq!(
            status.map(|status| (status.context_used, status.five_hour_used)),
            Some((None, None))
        );
    }

    #[test]
    fn the_users_status_line_command_is_read_from_their_settings() {
        assert_eq!(
            user_command(r#"{"statusLine":{"type":"command","command":" ~/.claude/line.sh "}}"#),
            Some("~/.claude/line.sh".to_owned())
        );
        assert_eq!(user_command(r#"{"statusLine":{"command":""}}"#), None);
        assert_eq!(user_command("not json"), None);
    }
}
