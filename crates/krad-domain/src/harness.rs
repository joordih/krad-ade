mod hooks;
mod inherited_env;
pub use hooks::*;
pub use inherited_env::INHERITED_SESSION_VARS;

use crate::agent::{AgentPreset, AgentState};
use crate::ids::ResumeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptDelivery {
    Argument,
    Flag(&'static str),
    AfterStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellQuoting {
    Posix,
    PowerShell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettledAsks {
    OfTool,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookSignal {
    Ready,
    Working,
    NeedsInput,
    Finished,
}

impl From<HookSignal> for AgentState {
    fn from(signal: HookSignal) -> Self {
        match signal {
            HookSignal::Ready => Self::Idle,
            HookSignal::Working => Self::Working,
            HookSignal::NeedsInput => Self::WaitingForInput,
            HookSignal::Finished => Self::Done,
        }
    }
}

const CLAUDE_QUESTION_TOOL: &str = "AskUserQuestion";
const CLAUDE_PLAN_TOOL: &str = "ExitPlanMode";
const CLAUDE_IDLE_NOTIFICATION: &str = "idle_prompt";
const CODEX_TURN_COMPLETE: &str = "agent-turn-complete";
const CLAUDE_SESSION_START: &str = "SessionStart";
const CLAUDE_PRE_TOOL_USE: &str = "PreToolUse";
const CLAUDE_POST_TOOL_USE: &str = "PostToolUse";
const CLAUDE_POST_TOOL_USE_FAILURE: &str = "PostToolUseFailure";
const CLAUDE_USER_PROMPT_SUBMIT: &str = "UserPromptSubmit";
const CLAUDE_NOTIFICATION: &str = "Notification";
const CLAUDE_PERMISSION_REQUEST: &str = "PermissionRequest";
const CLAUDE_STOP: &str = "Stop";
const CLAUDE_STOP_FAILURE: &str = "StopFailure";
const OPENCODE_SESSION_IDLE: &str = "session.idle";
const OPENCODE_SESSION_ERROR: &str = "session.error";
const OPENCODE_SESSION_STATUS: &str = "session.status";
const OPENCODE_EXECUTION_STARTED: &str = "session.execution.started";
const OPENCODE_EXECUTION_SUCCEEDED: &str = "session.execution.succeeded";
const OPENCODE_EXECUTION_FAILED: &str = "session.execution.failed";
const OPENCODE_EXECUTION_INTERRUPTED: &str = "session.execution.interrupted";
const OPENCODE_PERMISSION_ASKED: &str = "permission.asked";
const OPENCODE_PERMISSION_REPLIED: &str = "permission.replied";
const OPENCODE_QUESTION_ASKED: &str = "question.asked";
const OPENCODE_QUESTION_REPLIED: &str = "question.replied";
const OPENCODE_FORM_CREATED: &str = "form.created";
const OPENCODE_FORM_REPLIED: &str = "form.replied";
const OPENCODE_TOOL_BEFORE: &str = "tool.execute.before";
const CLAUDE_STARTUP_SOURCE: &str = "startup";
const CLAUDE_COMPACT_SOURCE: &str = "compact";
const CLAUDE_RESUME_FLAG: &str = "--resume";
const CODEX_RESUME_COMMAND: &str = "resume";
const CLAUDE_FORK_FLAG: &str = "--fork-session";
const POSIX_QUOTE_ESCAPE: &str = r"'\''";
const POWERSHELL_QUOTE_ESCAPE: &str = "''";
const QUOTE: char = '\'';
const DOUBLE_QUOTE: char = '"';
const BACKSLASH: char = '\\';
const TRAILING_GUARD: char = ' ';
const MAX_INLINE_LINE_BYTES: usize = 3500;

impl AgentPreset {
    #[must_use]
    pub const fn prompt_delivery(self) -> PromptDelivery {
        match self {
            Self::ClaudeCode | Self::Codex | Self::CursorAgent => PromptDelivery::Argument,
            Self::Opencode => PromptDelivery::Flag("--prompt"),
            Self::Gemini => PromptDelivery::Flag("--prompt-interactive"),
            Self::Copilot => PromptDelivery::Flag("-i"),
            Self::Shell | Self::Amp | Self::Aider | Self::Goose => PromptDelivery::AfterStart,
        }
    }

    #[must_use]
    pub fn hook_signal(self, event: &str, tool: Option<&str>) -> Option<HookSignal> {
        match self {
            Self::ClaudeCode => claude_signal(event, tool),
            Self::Codex => (event == CODEX_TURN_COMPLETE).then_some(HookSignal::Finished),
            Self::Opencode => opencode_signal(event),
            _ => None,
        }
    }
}

impl AgentPreset {
    #[must_use]
    pub fn settled_asks(self, event: &str) -> Option<SettledAsks> {
        if self != Self::ClaudeCode {
            return None;
        }
        match event {
            CLAUDE_POST_TOOL_USE | CLAUDE_POST_TOOL_USE_FAILURE => Some(SettledAsks::OfTool),
            CLAUDE_STOP
            | CLAUDE_STOP_FAILURE
            | CLAUDE_USER_PROMPT_SUBMIT
            | CLAUDE_SESSION_START => Some(SettledAsks::All),
            _ => None,
        }
    }

    #[must_use]
    pub const fn resume_flag(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some(CLAUDE_RESUME_FLAG),
            Self::Codex => Some(CODEX_RESUME_COMMAND),
            _ => None,
        }
    }

    #[must_use]
    pub const fn question_tool(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some(CLAUDE_QUESTION_TOOL),
            _ => None,
        }
    }

    #[must_use]
    pub const fn plan_tool(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some(CLAUDE_PLAN_TOOL),
            _ => None,
        }
    }

    #[must_use]
    pub const fn fork_flag(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some(CLAUDE_FORK_FLAG),
            _ => None,
        }
    }

    #[must_use]
    pub fn started_fresh(self, event: &str, kind: Option<&str>) -> bool {
        self == Self::ClaudeCode
            && event == CLAUDE_SESSION_START
            && kind == Some(CLAUDE_STARTUP_SOURCE)
    }
}

#[must_use]
pub fn resume_line(
    line: &str,
    preset: AgentPreset,
    id: &ResumeId,
    quoting: ShellQuoting,
) -> String {
    let Some(flag) = preset.resume_flag() else {
        return line.to_owned();
    };
    let resumed = format!("{line} {flag} {id}");
    match quoting {
        ShellQuoting::Posix => format!("{resumed} || {line}"),
        ShellQuoting::PowerShell => format!("{resumed}; if (-not $?) {{ {line} }}"),
    }
}

fn claude_signal(event: &str, tool: Option<&str>) -> Option<HookSignal> {
    match event {
        CLAUDE_PRE_TOOL_USE if tool == Some(CLAUDE_QUESTION_TOOL) => Some(HookSignal::NeedsInput),
        CLAUDE_USER_PROMPT_SUBMIT
        | CLAUDE_PRE_TOOL_USE
        | CLAUDE_POST_TOOL_USE
        | CLAUDE_POST_TOOL_USE_FAILURE => Some(HookSignal::Working),
        CLAUDE_NOTIFICATION if tool == Some(CLAUDE_IDLE_NOTIFICATION) => None,
        CLAUDE_PERMISSION_REQUEST | CLAUDE_NOTIFICATION => Some(HookSignal::NeedsInput),
        CLAUDE_STOP | CLAUDE_STOP_FAILURE => Some(HookSignal::Finished),
        CLAUDE_SESSION_START if tool == Some(CLAUDE_COMPACT_SOURCE) => None,
        CLAUDE_SESSION_START => Some(HookSignal::Ready),
        _ => None,
    }
}

fn opencode_signal(event: &str) -> Option<HookSignal> {
    match event {
        OPENCODE_SESSION_IDLE
        | OPENCODE_EXECUTION_SUCCEEDED
        | OPENCODE_EXECUTION_FAILED
        | OPENCODE_EXECUTION_INTERRUPTED
        | OPENCODE_SESSION_ERROR => Some(HookSignal::Finished),
        OPENCODE_PERMISSION_ASKED | OPENCODE_QUESTION_ASKED | OPENCODE_FORM_CREATED => {
            Some(HookSignal::NeedsInput)
        }
        OPENCODE_EXECUTION_STARTED
        | OPENCODE_SESSION_STATUS
        | OPENCODE_TOOL_BEFORE
        | OPENCODE_PERMISSION_REPLIED
        | OPENCODE_QUESTION_REPLIED
        | OPENCODE_FORM_REPLIED => Some(HookSignal::Working),
        _ => None,
    }
}

#[must_use]
pub fn quote_for_shell(text: &str, quoting: ShellQuoting) -> String {
    let flat = text.replace(['\r', '\n'], " ");
    let (flat, escape) = match quoting {
        ShellQuoting::Posix => (flat, POSIX_QUOTE_ESCAPE),
        ShellQuoting::PowerShell => (escape_for_native(&flat), POWERSHELL_QUOTE_ESCAPE),
    };
    format!("{QUOTE}{}{QUOTE}", flat.replace(QUOTE, escape))
}

fn escape_for_native(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut backslashes = 0;
    for ch in text.chars() {
        if ch == DOUBLE_QUOTE {
            out.extend(std::iter::repeat_n(BACKSLASH, backslashes + 1));
        }
        backslashes = if ch == BACKSLASH { backslashes + 1 } else { 0 };
        out.push(ch);
    }
    if backslashes > 0 {
        out.push(TRAILING_GUARD);
    }
    out
}

#[must_use]
pub fn launch_with_prompt(
    line: &str,
    preset: AgentPreset,
    prompt: &str,
    quoting: ShellQuoting,
) -> (String, Option<String>) {
    if prompt.trim().is_empty() {
        return (line.to_owned(), None);
    }
    let inline = match preset.prompt_delivery() {
        PromptDelivery::Argument => format!("{line} {}", quote_for_shell(prompt, quoting)),
        PromptDelivery::Flag(flag) => format!("{line} {flag} {}", quote_for_shell(prompt, quoting)),
        PromptDelivery::AfterStart => return after_start(line, prompt),
    };
    if inline.len() > MAX_INLINE_LINE_BYTES {
        return after_start(line, prompt);
    }
    (inline, None)
}

fn after_start(line: &str, prompt: &str) -> (String, Option<String>) {
    (line.to_owned(), Some(prompt.replace(['\r', '\n'], " ")))
}

const WORKER_BRIEF_OPENING: &str = "You are a worker agent started by ";

#[must_use]
pub fn worker_brief(coordinator: u64, cli: &str, task: &str) -> String {
    format!(
        "{WORKER_BRIEF_OPENING}terminal {coordinator} in Krad ADE. \
         Do this task: {task} \
         When you finish, report once with: {cli} send {coordinator} \"<what you did, in three sentences>\". \
         If you are blocked, ask with: {cli} send {coordinator} \"Blocked: <reason>\" and then {cli} check --wait 600."
    )
}

#[must_use]
pub fn chat_worker_brief(task: &str) -> String {
    format!(
        "{WORKER_BRIEF_OPENING}the Krad ADE chat, which follows your screen. \
         Do this task: {task} \
         When you finish, end with a short summary of what you did and how you checked it. \
         If you are blocked, say why and stop; the chat will answer you."
    )
}

#[must_use]
pub fn is_worker_brief(prompt: &str) -> bool {
    prompt.starts_with(WORKER_BRIEF_OPENING)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_that_ran_or_a_turn_that_ended_settles_the_held_asks() {
        let claude = AgentPreset::ClaudeCode;
        assert_eq!(
            claude.settled_asks("PostToolUse"),
            Some(SettledAsks::OfTool)
        );
        assert_eq!(claude.settled_asks("Stop"), Some(SettledAsks::All));
        assert_eq!(claude.settled_asks("PermissionRequest"), None);
        assert_eq!(AgentPreset::Codex.settled_asks("Stop"), None);
    }

    #[test]
    fn worker_briefs_are_told_apart_from_user_prompts() {
        assert!(is_worker_brief(&worker_brief(
            3,
            "krad-ade-agents",
            "fix it"
        )));
        assert!(is_worker_brief(&chat_worker_brief("fix it")));
        assert!(!is_worker_brief("fix the webhook"));
    }

    #[test]
    fn claude_idle_notifications_leave_the_state_alone() {
        let claude = AgentPreset::ClaudeCode;
        assert_eq!(
            claude.hook_signal("Notification", Some(CLAUDE_IDLE_NOTIFICATION)),
            None
        );
        assert_eq!(
            claude.hook_signal("Notification", Some("permission_prompt")),
            Some(HookSignal::NeedsInput)
        );
        assert_eq!(
            claude.hook_signal("Notification", None),
            Some(HookSignal::NeedsInput),
            "older Claude Code sends no type; a notification still means it asks"
        );
    }

    #[test]
    fn claude_hooks_map_like_orca() {
        let claude = AgentPreset::ClaudeCode;
        assert_eq!(
            claude.hook_signal("UserPromptSubmit", None),
            Some(HookSignal::Working)
        );
        assert_eq!(
            claude.hook_signal("PreToolUse", Some("Bash")),
            Some(HookSignal::Working)
        );
        assert_eq!(
            claude.hook_signal("PreToolUse", Some(CLAUDE_QUESTION_TOOL)),
            Some(HookSignal::NeedsInput)
        );
        assert_eq!(
            claude.hook_signal("PermissionRequest", None),
            Some(HookSignal::NeedsInput)
        );
        assert_eq!(claude.hook_signal("Stop", None), Some(HookSignal::Finished));
        assert_eq!(
            claude.hook_signal("SessionStart", Some("resume")),
            Some(HookSignal::Ready)
        );
        assert_eq!(
            claude.hook_signal("SessionStart", Some(CLAUDE_COMPACT_SOURCE)),
            None,
            "compaction happens inside a turn"
        );
        assert_eq!(
            AgentPreset::Codex.hook_signal(CODEX_TURN_COMPLETE, None),
            Some(HookSignal::Finished)
        );
        assert_eq!(
            AgentPreset::Opencode.hook_signal("session.idle", None),
            Some(HookSignal::Finished)
        );
        assert_eq!(
            AgentPreset::Opencode.hook_signal("session.execution.succeeded", None),
            Some(HookSignal::Finished)
        );
        assert_eq!(
            AgentPreset::Opencode.hook_signal("question.asked", None),
            Some(HookSignal::NeedsInput)
        );
    }

    #[test]
    fn claude_resumes_by_id_and_falls_back_to_a_fresh_conversation() {
        let parsed = "0b7c3a52-9f1e".parse::<ResumeId>();
        assert!(parsed.is_ok(), "a uuid-like id parses");
        let Ok(id) = parsed else {
            return;
        };
        assert_eq!(
            resume_line("claude", AgentPreset::ClaudeCode, &id, ShellQuoting::Posix),
            "claude --resume 0b7c3a52-9f1e || claude"
        );
        assert_eq!(
            resume_line(
                "claude",
                AgentPreset::ClaudeCode,
                &id,
                ShellQuoting::PowerShell
            ),
            "claude --resume 0b7c3a52-9f1e; if (-not $?) { claude }"
        );
        assert_eq!(
            resume_line("codex", AgentPreset::Codex, &id, ShellQuoting::Posix),
            "codex resume 0b7c3a52-9f1e || codex"
        );
        assert_eq!(
            resume_line("opencode", AgentPreset::Opencode, &id, ShellQuoting::Posix),
            "opencode"
        );
    }

    #[test]
    fn only_a_claude_startup_is_a_fresh_conversation() {
        let claude = AgentPreset::ClaudeCode;
        assert!(claude.started_fresh(CLAUDE_SESSION_START, Some(CLAUDE_STARTUP_SOURCE)));
        assert!(!claude.started_fresh(CLAUDE_SESSION_START, Some("resume")));
        assert!(!claude.started_fresh("Stop", None));
        assert!(
            !AgentPreset::Codex.started_fresh(CLAUDE_SESSION_START, Some(CLAUDE_STARTUP_SOURCE))
        );
    }

    #[test]
    fn prompts_are_quoted_for_each_shell() {
        let prompt = "fix Bob's\ntest";
        assert_eq!(
            launch_with_prompt(
                "claude",
                AgentPreset::ClaudeCode,
                prompt,
                ShellQuoting::Posix
            ),
            (r"claude 'fix Bob'\''s test'".to_owned(), None)
        );
        assert_eq!(
            launch_with_prompt(
                "opencode",
                AgentPreset::Opencode,
                prompt,
                ShellQuoting::PowerShell
            ),
            ("opencode --prompt 'fix Bob''s test'".to_owned(), None)
        );
        assert_eq!(
            launch_with_prompt("aider", AgentPreset::Aider, prompt, ShellQuoting::Posix),
            ("aider".to_owned(), Some("fix Bob's test".to_owned()))
        );
        assert_eq!(
            launch_with_prompt("codex", AgentPreset::Codex, "  ", ShellQuoting::Posix),
            ("codex".to_owned(), None)
        );
    }

    #[test]
    fn long_prompts_are_typed_after_start() {
        let prompt = "x".repeat(MAX_INLINE_LINE_BYTES);
        for (line, preset) in [
            ("claude", AgentPreset::ClaudeCode),
            ("codex", AgentPreset::Codex),
            ("opencode", AgentPreset::Opencode),
            ("gemini", AgentPreset::Gemini),
        ] {
            for quoting in [ShellQuoting::Posix, ShellQuoting::PowerShell] {
                assert_eq!(
                    launch_with_prompt(line, preset, &prompt, quoting),
                    (line.to_owned(), Some(prompt.clone())),
                    "{line}"
                );
            }
        }
    }

    #[test]
    fn prompts_with_double_quotes_stay_on_the_powershell_line() {
        let prompt = r#"write {"version":1} to "out.json""#;
        assert_eq!(
            launch_with_prompt(
                "claude",
                AgentPreset::ClaudeCode,
                prompt,
                ShellQuoting::PowerShell
            ),
            (
                r#"claude 'write {\"version\":1} to \"out.json\"'"#.to_owned(),
                None
            )
        );
        let (line, typed) = launch_with_prompt(
            "claude",
            AgentPreset::ClaudeCode,
            prompt,
            ShellQuoting::Posix,
        );
        assert_eq!(line, format!("claude '{prompt}'"));
        assert_eq!(typed, None);
    }

    #[test]
    fn powershell_quoting_survives_the_native_command_line() {
        let cases = [
            ("plain words", "'plain words'"),
            (r#"{"k":"v"}"#, r#"'{\"k\":\"v\"}'"#),
            (r#"C:\a\"b\""#, r#"'C:\a\\\"b\\\"'"#),
            ("it's", "'it''s'"),
            (r"C:\dir\", r"'C:\dir\ '"),
            (r"just \ back \\ slashes", r"'just \ back \\ slashes'"),
            ("two\nlines", "'two lines'"),
        ];
        for (text, quoted) in cases {
            assert_eq!(
                quote_for_shell(text, ShellQuoting::PowerShell),
                quoted,
                "{text}"
            );
        }
    }
}
