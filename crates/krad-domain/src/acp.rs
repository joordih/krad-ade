mod id;
mod offers;
mod secrets;
mod settings;

pub use id::{AcpAgentId, AcpAgentIdError, MAX_ACP_AGENT_ID_LEN};
pub use offers::{
    AcpAuthMethod, AcpAuthMethodId, AcpConfigChoice, AcpConfigOption, AcpConfigOptionId,
    AcpConfigValue, AcpMode, AcpModeId, AcpModel, AcpModelId, AcpModels, AcpModes,
    AcpPermissionKind, AcpPermissionOption, AcpPermissionOptionId, AcpSignIn,
};
pub use secrets::{AcpAgentStart, AcpSecretKey, EnteredEnvValue, EnteredEnvVariable, SplitEnv};
pub use settings::{AcpAgentRejected, AcpSettings};

use crate::mcp::McpVariable;
use serde::{Deserialize, Serialize};

pub const ACP_DISPLAY_SUFFIX: &str = " (ACP)";

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AcpAgent {
    pub id: AcpAgentId,
    pub name: String,
    pub program: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<McpVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
}

impl AcpAgent {
    #[must_use]
    pub fn display_name(&self) -> String {
        format!("{}{ACP_DISPLAY_SUFFIX}", self.name)
    }

    #[must_use]
    pub fn is_built_in(&self) -> bool {
        is_built_in_acp_agent(self.id)
    }
}

struct BuiltInAcpAgent {
    id: &'static str,
    name: &'static str,
    program: &'static str,
    args: &'static [&'static str],
    terminal: &'static str,
}

impl BuiltInAcpAgent {
    fn agent(&self) -> Option<AcpAgent> {
        Some(AcpAgent {
            id: self.id.parse().ok()?,
            name: self.name.to_owned(),
            program: self.program.to_owned(),
            args: self.args.iter().map(|&arg| arg.to_owned()).collect(),
            env: Vec::new(),
            terminal: Some(self.terminal.to_owned()),
        })
    }
}

const BUILT_IN_ACP_AGENTS: [BuiltInAcpAgent; 3] = [
    BuiltInAcpAgent {
        id: "claude-code-acp",
        name: "Claude Code",
        program: "claude-code-acp",
        args: &[],
        terminal: "claude",
    },
    BuiltInAcpAgent {
        id: "codex-acp",
        name: "Codex",
        program: "codex-acp",
        args: &[],
        terminal: "codex",
    },
    BuiltInAcpAgent {
        id: "opencode",
        name: "opencode",
        program: "opencode",
        args: &["acp"],
        terminal: "opencode",
    },
];

#[must_use]
pub fn built_in_acp_agents() -> Vec<AcpAgent> {
    BUILT_IN_ACP_AGENTS
        .iter()
        .filter_map(BuiltInAcpAgent::agent)
        .collect()
}

#[must_use]
pub fn is_built_in_acp_agent(id: AcpAgentId) -> bool {
    BUILT_IN_ACP_AGENTS
        .iter()
        .any(|built_in| built_in.id == id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Listed<'a> = (&'a str, &'a str, Vec<&'a str>, Option<&'a str>, String);

    #[test]
    fn the_built_in_agents_are_the_acp_adapters_of_the_known_harnesses() {
        let agents = built_in_acp_agents();
        let listed: Vec<Listed<'_>> = agents
            .iter()
            .map(|agent| {
                (
                    agent.id.as_str(),
                    agent.program.as_str(),
                    agent.args.iter().map(String::as_str).collect(),
                    agent.terminal.as_deref(),
                    agent.display_name(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                (
                    "claude-code-acp",
                    "claude-code-acp",
                    vec![],
                    Some("claude"),
                    "Claude Code (ACP)".to_owned()
                ),
                (
                    "codex-acp",
                    "codex-acp",
                    vec![],
                    Some("codex"),
                    "Codex (ACP)".to_owned()
                ),
                (
                    "opencode",
                    "opencode",
                    vec!["acp"],
                    Some("opencode"),
                    "opencode (ACP)".to_owned()
                ),
            ]
        );
        assert!(agents.iter().all(AcpAgent::is_built_in));
    }
}
