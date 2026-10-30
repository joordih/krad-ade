use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Working,
    WaitingForInput,
    Done,
}

impl AgentState {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::WaitingForInput => "waiting-for-input",
            AgentState::Done => "done",
        }
    }
}

pub const AGENT_WAIT_SECS: f32 = 3.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum AgentPreset {
    #[default]
    #[serde(
        alias = "Gemini",
        alias = "Copilot",
        alias = "CursorAgent",
        alias = "Amp",
        alias = "Aider",
        alias = "Goose"
    )]
    Shell,
    ClaudeCode,
    Codex,
    Opencode,
    Gateway,
}

impl AgentPreset {
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::Shell,
            Self::ClaudeCode,
            Self::Codex,
            Self::Opencode,
            Self::Gateway,
        ]
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Shell => "Shell",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Opencode => "opencode",
            Self::Gateway => "Gateway",
        }
    }

    #[must_use]
    pub const fn program(self) -> Option<&'static str> {
        match self {
            Self::Shell | Self::Gateway => None,
            Self::ClaudeCode => Some("claude"),
            Self::Codex => Some("codex"),
            Self::Opencode => Some("opencode"),
        }
    }

    #[must_use]
    pub const fn system_prompt_flag(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some("--append-system-prompt"),
            _ => None,
        }
    }

    #[must_use]
    pub const fn model_flag(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some("--model"),
            Self::Codex | Self::Opencode => Some("-m"),
            Self::Shell | Self::Gateway => None,
        }
    }

    #[must_use]
    pub const fn is_agent(self) -> bool {
        self.program().is_some()
    }
}

#[must_use]
pub fn classify_agent_state(
    idle_secs: f32,
    child_alive: bool,
    bell: bool,
    is_agent: bool,
) -> AgentState {
    if !child_alive || !is_agent {
        return AgentState::Idle;
    }
    if bell || idle_secs >= AGENT_WAIT_SECS {
        return AgentState::WaitingForInput;
    }
    AgentState::Working
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_has_its_own_program() {
        let programs: Vec<&str> = AgentPreset::all()
            .iter()
            .filter_map(|preset| preset.program())
            .collect();
        let unique: std::collections::HashSet<&&str> = programs.iter().collect();
        assert_eq!(unique.len(), programs.len());
        assert_eq!(programs.len(), AgentPreset::all().len() - 2);
        assert!(!AgentPreset::Shell.is_agent());
        assert!(!AgentPreset::Gateway.is_agent());
    }

    #[test]
    fn presets_of_dropped_agents_read_back_as_a_shell() -> serde_json::Result<()> {
        let read: Vec<AgentPreset> = serde_json::from_str(r#"["Gemini","Aider","Codex"]"#)?;
        assert_eq!(
            read,
            [AgentPreset::Shell, AgentPreset::Shell, AgentPreset::Codex]
        );
        Ok(())
    }

    #[test]
    fn agents_work_while_printing_and_wait_once_silent() {
        assert_eq!(
            classify_agent_state(0.5, true, false, true),
            AgentState::Working
        );
        assert_eq!(
            classify_agent_state(0.5, true, true, true),
            AgentState::WaitingForInput,
            "a bell asks for the user at once"
        );
        assert_eq!(
            classify_agent_state(AGENT_WAIT_SECS, true, false, true),
            AgentState::WaitingForInput,
            "a TUI sitting silent at its prompt"
        );
        assert_eq!(
            classify_agent_state(0.1, true, false, false),
            AgentState::Idle,
            "a shell printing its prompt is not working"
        );
        assert_eq!(
            classify_agent_state(0.1, false, false, true),
            AgentState::Idle
        );
    }
}
