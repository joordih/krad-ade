use super::{quote_for_shell, ShellQuoting};
use super::{CLAUDE_PERMISSION_REQUEST, CLAUDE_PRE_TOOL_USE, CLAUDE_QUESTION_TOOL};
use crate::agent::AgentPreset;

const CLAUDE_EVENTS: [&str; 6] = [
    "UserPromptSubmit",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionStart",
];
const CLAUDE_TOOL_EVENTS: [&str; 3] = ["PreToolUse", "PostToolUse", "PostToolUseFailure"];
const CLAUDE_MATCH_ALL: &str = "*";
const CLAUDE_HOOK_TIMEOUT_SECS: u64 = 10;
pub const ASK_WAIT_SECS: u64 = 1800;
const CLAUDE_ASK_TIMEOUT_SECS: u64 = ASK_WAIT_SECS + 30;
const HOOK_VERB: &str = "hook";
const ASK_VERB: &str = "ask";
const STATUS_LINE_VERB: &str = "statusline";
const STATUS_LINE_TYPE: &str = "command";
const USER_SETTINGS: &str = "$HOME/.claude/settings.json";
const CODEX_BUS_NETWORK: &str = "sandbox_workspace_write.network_access=true";

#[must_use]
pub fn claude_hook_settings(cli: &str) -> String {
    let program = AgentPreset::ClaudeCode.program().unwrap_or_default();
    let command = format!("{cli} {HOOK_VERB} {program}");
    let ask = format!("{cli} {ASK_VERB} {program}");
    let hook = |command: &str, timeout: u64, matcher: Option<&str>| {
        let mut hook = serde_json::json!({
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": timeout,
            }]
        });
        if let Some(matcher) = matcher {
            hook["matcher"] = serde_json::Value::from(matcher);
        }
        hook
    };
    let mut hooks = serde_json::Map::new();
    for event in CLAUDE_EVENTS {
        let reports = hook(&command, CLAUDE_HOOK_TIMEOUT_SECS, None);
        hooks.insert(event.to_owned(), serde_json::Value::Array(vec![reports]));
    }
    for event in CLAUDE_TOOL_EVENTS {
        let reports = hook(&command, CLAUDE_HOOK_TIMEOUT_SECS, Some(CLAUDE_MATCH_ALL));
        hooks.insert(event.to_owned(), serde_json::Value::Array(vec![reports]));
    }
    let asks = [
        (CLAUDE_PRE_TOOL_USE, Some(CLAUDE_QUESTION_TOOL)),
        (CLAUDE_PERMISSION_REQUEST, None),
    ];
    for (event, matcher) in asks {
        if let Some(serde_json::Value::Array(entries)) = hooks.get_mut(event) {
            entries.push(hook(&ask, CLAUDE_ASK_TIMEOUT_SECS, matcher));
        }
    }
    serde_json::json!({
        "hooks": hooks,
        "statusLine": {
            "type": STATUS_LINE_TYPE,
            "command": status_line_command(cli, program),
        },
    })
    .to_string()
}

fn status_line_command(cli: &str, program: &str) -> String {
    format!(
        r#"i=$(cat); u=$(printf '%s' "$i" | {cli} {STATUS_LINE_VERB} {program} "$(cat "{USER_SETTINGS}" 2>/dev/null)"); [ -z "$u" ] || printf '%s' "$i" | sh -c "$u""#
    )
}

#[must_use]
pub fn hook_launch_args(
    preset: AgentPreset,
    cli: &str,
    quoting: ShellQuoting,
    settings_file: Option<&str>,
) -> Option<String> {
    let program = preset.program()?;
    match preset {
        AgentPreset::ClaudeCode => {
            let settings = settings_file.map_or_else(|| claude_hook_settings(cli), str::to_owned);
            Some(format!(
                "--settings {}",
                quote_for_shell(&settings, quoting)
            ))
        }
        AgentPreset::Codex => {
            let notify = format!("notify=['{cli}','{HOOK_VERB}','{program}']");
            Some(format!(
                "-c {} -c {CODEX_BUS_NETWORK}",
                quote_for_shell(&notify, quoting)
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_launch_lines_carry_no_double_quote() -> serde_json::Result<()> {
        let claude = hook_launch_args(
            AgentPreset::ClaudeCode,
            "krad-ade-agents",
            ShellQuoting::PowerShell,
            Some(r"C:\Users\Ana Perez\krad-ade\claude-hooks.json"),
        );
        assert_eq!(
            claude.as_deref(),
            Some(r"--settings 'C:\Users\Ana Perez\krad-ade\claude-hooks.json'")
        );
        let codex = hook_launch_args(
            AgentPreset::Codex,
            "krad-ade-agents",
            ShellQuoting::PowerShell,
            None,
        );
        assert!(codex.is_some_and(|args| !args.contains('"')));
        let settings: serde_json::Value =
            serde_json::from_str(&claude_hook_settings("krad-ade-agents"))?;
        assert_eq!(
            settings["hooks"]["Stop"][0]["hooks"][0]["command"],
            "krad-ade-agents hook claude"
        );
        Ok(())
    }

    #[test]
    fn the_status_line_reports_to_the_app_and_keeps_the_users_own() -> serde_json::Result<()> {
        let settings: serde_json::Value =
            serde_json::from_str(&claude_hook_settings("krad-ade-agents.exe"))?;
        let command = settings["statusLine"]["command"]
            .as_str()
            .unwrap_or_default();
        assert_eq!(settings["statusLine"]["type"], "command");
        assert!(command.contains("krad-ade-agents.exe statusline claude"));
        assert!(command.contains(USER_SETTINGS));
        assert!(command.ends_with(r#"sh -c "$u""#));
        Ok(())
    }
}
