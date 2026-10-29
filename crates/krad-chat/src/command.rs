use krad_domain::{AgentPreset, ModelId, ResumeId};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::control::Approvals;
use crate::live::LiveConfig;

const CLAUDE_ARGS: &[&str] = &["-p", "--output-format", "stream-json", "--verbose"];
const LIVE_CLAUDE_ARGS: &[&str] = &[
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
];
const CLAUDE_SESSION_ID: &str = "--session-id";
const CLAUDE_PERMISSION_PROMPT_TOOL: &str = "--permission-prompt-tool";
const CLAUDE_STDIO_PROMPT_TOOL: &str = "stdio";
const CLAUDE_EFFORT: &str = "--effort";
const CLAUDE_PERMISSION_MODE: &str = "--permission-mode";
const CODEX_ARGS: &[&str] = &["exec", "--json", "--skip-git-repo-check"];
const CODEX_STDIN_PROMPT: &str = "-";
const OPENCODE_ARGS: &[&str] = &["run", "--format", "json"];
const GEMINI_ARGS: &[&str] = &["--output-format", "stream-json"];
const CLAUDE_ADD_DIR: &str = "--add-dir";
const GEMINI_ADD_DIR: &str = "--include-directories";
const CLAUDE_ALLOWED_TOOLS: &str = "--allowedTools";
const CLAUDE_BASH_DEFAULT_TIMEOUT: &str = "BASH_DEFAULT_TIMEOUT_MS";
const CLAUDE_BASH_MAX_TIMEOUT: &str = "BASH_MAX_TIMEOUT_MS";
const ORCHESTRATOR_BASH_TIMEOUT: Duration = Duration::from_mins(31);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatCommand {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct BusVars(pub Vec<(String, String)>);

impl fmt::Debug for BusVars {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entries(self.0.iter().map(|(name, _)| name))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Orchestration {
    pub cli: String,
    pub env: BusVars,
}

#[must_use]
pub const fn supports_orchestration(preset: AgentPreset) -> bool {
    matches!(preset, AgentPreset::ClaudeCode)
}

#[must_use]
pub const fn supports_live_chat(preset: AgentPreset) -> bool {
    matches!(preset, AgentPreset::ClaudeCode)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveSession {
    Start(ResumeId),
    Resume(ResumeId),
    Fork(ResumeId),
}

impl LiveSession {
    #[must_use]
    pub const fn history(&self) -> Option<&ResumeId> {
        match self {
            Self::Start(_) => None,
            Self::Resume(id) | Self::Fork(id) => Some(id),
        }
    }
}

#[must_use]
pub const fn supports_chat(preset: AgentPreset) -> bool {
    matches!(
        preset,
        AgentPreset::ClaudeCode | AgentPreset::Codex | AgentPreset::Opencode | AgentPreset::Gemini
    )
}

const fn add_dir_flag(preset: AgentPreset) -> Option<&'static str> {
    match preset {
        AgentPreset::ClaudeCode => Some(CLAUDE_ADD_DIR),
        AgentPreset::Gemini => Some(GEMINI_ADD_DIR),
        _ => None,
    }
}

#[must_use]
pub fn chat_command(
    preset: AgentPreset,
    model: Option<&ModelId>,
    extra_dirs: &[PathBuf],
    orchestrator_cli: Option<&str>,
) -> Option<ChatCommand> {
    let base: &[&str] = match preset {
        AgentPreset::ClaudeCode => CLAUDE_ARGS,
        AgentPreset::Codex => CODEX_ARGS,
        AgentPreset::Opencode => OPENCODE_ARGS,
        AgentPreset::Gemini => GEMINI_ARGS,
        AgentPreset::Shell
        | AgentPreset::Copilot
        | AgentPreset::CursorAgent
        | AgentPreset::Amp
        | AgentPreset::Aider
        | AgentPreset::Goose => return None,
    };
    let mut command = harness_args(preset, base, model, extra_dirs, orchestrator_cli);
    if preset == AgentPreset::Codex {
        command.args.push(CODEX_STDIN_PROMPT.to_owned());
    }
    Some(command)
}

#[must_use]
pub fn live_chat_command(config: &LiveConfig, session: &LiveSession) -> ChatCommand {
    let claude = AgentPreset::ClaudeCode;
    let mut command = harness_args(
        claude,
        LIVE_CLAUDE_ARGS,
        config.model.as_ref(),
        &config.extra_dirs,
        config
            .orchestration
            .as_ref()
            .map(|orchestration| orchestration.cli.as_str()),
    );
    let args = &mut command.args;
    let (flag, id) = match session {
        LiveSession::Start(id) => (Some(CLAUDE_SESSION_ID), id),
        LiveSession::Resume(id) | LiveSession::Fork(id) => (claude.resume_flag(), id),
    };
    if let Some(flag) = flag {
        args.extend([flag.to_owned(), id.as_str().to_owned()]);
    }
    if matches!(session, LiveSession::Fork(_)) {
        args.extend(claude.fork_flag().map(str::to_owned));
    }
    if config.approvals == Approvals::Asked {
        args.extend([
            CLAUDE_PERMISSION_PROMPT_TOOL.to_owned(),
            CLAUDE_STDIO_PROMPT_TOOL.to_owned(),
        ]);
    }
    if let (Some(prompt), Some(flag)) = (&config.system_prompt, claude.system_prompt_flag()) {
        args.extend([flag.to_owned(), prompt.clone()]);
    }
    if let Some(effort) = config.effort {
        args.extend([CLAUDE_EFFORT.to_owned(), effort.to_string()]);
    }
    if let Some(mode) = config.permission_mode {
        args.extend([CLAUDE_PERMISSION_MODE.to_owned(), mode.to_string()]);
    }
    command
}

fn harness_args(
    preset: AgentPreset,
    base: &[&str],
    model: Option<&ModelId>,
    extra_dirs: &[PathBuf],
    orchestrator_cli: Option<&str>,
) -> ChatCommand {
    let mut args: Vec<String> = base.iter().map(|arg| (*arg).to_owned()).collect();
    if let (Some(model), Some(flag)) = (model, preset.model_flag()) {
        args.push(flag.to_owned());
        args.push(model.as_str().to_owned());
    }
    if let Some(flag) = add_dir_flag(preset) {
        for dir in extra_dirs {
            args.push(flag.to_owned());
            args.push(dir.to_string_lossy().into_owned());
        }
    }
    let mut env = Vec::new();
    if let Some(cli) = orchestrator_cli.filter(|_| supports_orchestration(preset)) {
        args.push(CLAUDE_ALLOWED_TOOLS.to_owned());
        args.push(format!("Bash({cli}:*)"));
        let timeout = ORCHESTRATOR_BASH_TIMEOUT.as_millis().to_string();
        env.push((CLAUDE_BASH_DEFAULT_TIMEOUT.to_owned(), timeout.clone()));
        env.push((CLAUDE_BASH_MAX_TIMEOUT.to_owned(), timeout));
    }
    ChatCommand { args, env }
}

#[cfg(test)]
mod tests;
