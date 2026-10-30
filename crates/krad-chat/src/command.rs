use krad_domain::{AgentPreset, ResumeId};
use std::fmt;

use crate::live::LiveConfig;
use crate::rewind::ResumeCut;

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
const CLAUDE_RESUME_AT: &str = "--resume-session-at";
const CLAUDE_DROPS_TURN: &str = "--resume-drops-turn";
const FLAG_VALUE_SEPARATOR: char = '=';

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
pub enum LiveSession {
    Start(ResumeId),
    Resume(ResumeId),
    Fork(ResumeId),
    Cut(ResumeCut),
}

impl LiveSession {
    #[must_use]
    pub const fn history(&self) -> Option<&ResumeId> {
        match self {
            Self::Start(_) => None,
            Self::Resume(id) | Self::Fork(id) | Self::Cut(ResumeCut { from: id, .. }) => Some(id),
        }
    }
}

#[must_use]
pub(crate) fn live_chat_command(config: &LiveConfig, session: &LiveSession) -> Vec<String> {
    let claude = AgentPreset::ClaudeCode;
    let mut args: Vec<String> = LIVE_CLAUDE_ARGS
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
    if let (Some(model), Some(flag)) = (&config.model, claude.model_flag()) {
        args.extend([flag.to_owned(), model.as_str().to_owned()]);
    }
    let (flag, id) = match session {
        LiveSession::Start(id) => (Some(CLAUDE_SESSION_ID), id),
        LiveSession::Resume(id)
        | LiveSession::Fork(id)
        | LiveSession::Cut(ResumeCut { from: id, .. }) => (claude.resume_flag(), id),
    };
    if let Some(flag) = flag {
        args.extend([flag.to_owned(), id.as_str().to_owned()]);
    }
    if let LiveSession::Cut(cut) = session {
        args.extend(cut_args(cut));
    }
    if matches!(session, LiveSession::Fork(_) | LiveSession::Cut(_)) {
        args.extend(claude.fork_flag().map(str::to_owned));
    }
    args.extend([
        CLAUDE_PERMISSION_PROMPT_TOOL.to_owned(),
        CLAUDE_STDIO_PROMPT_TOOL.to_owned(),
    ]);
    if let (Some(prompt), Some(flag)) = (&config.system_prompt, claude.system_prompt_flag()) {
        args.extend([flag.to_owned(), prompt.clone()]);
    }
    if let Some(effort) = config.effort {
        args.extend([CLAUDE_EFFORT.to_owned(), effort.to_string()]);
    }
    if let Some(mode) = config.permission_mode {
        args.extend([CLAUDE_PERMISSION_MODE.to_owned(), mode.to_string()]);
    }
    args
}

fn cut_args(cut: &ResumeCut) -> impl Iterator<Item = String> + '_ {
    let flagged = |flag: &str, value: &str| format!("{flag}{FLAG_VALUE_SEPARATOR}{value}");
    std::iter::once(flagged(CLAUDE_RESUME_AT, cut.at.as_str())).chain(
        cut.drops
            .iter()
            .map(move |drops| flagged(CLAUDE_DROPS_TURN, drops.as_str())),
    )
}

#[cfg(test)]
mod tests;
