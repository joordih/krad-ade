mod adapter;
mod command_line;
mod controls;
mod id;
mod offers;
mod package;
mod secrets;
mod settings;

pub use adapter::{AdapterFound, AdapterStart, AdapterWork, ManagedAdapter};
pub use command_line::CommandLine;
pub use controls::{AcpEffort, AcpOptionRole};
pub use id::{AcpAgentId, AcpAgentIdError, MAX_ACP_AGENT_ID_LEN};
pub use offers::{
    AcpAuthMethod, AcpAuthMethodId, AcpConfigCategory, AcpConfigChoice, AcpConfigOption,
    AcpConfigOptionId, AcpConfigValue, AcpMode, AcpModeId, AcpModel, AcpModelId, AcpModels,
    AcpModes, AcpPermissionKind, AcpPermissionOption, AcpPermissionOptionId, AcpSignIn,
};
pub use package::{AcpPackage, AcpPackageState, NpmPackage};
pub use secrets::{AcpAgentStart, AcpSecretKey, EnteredEnvValue, EnteredEnvVariable, SplitEnv};
pub use settings::{AcpAgentRejected, AcpSettings};

use crate::agent::AgentPreset;
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
        if acp_stand_in_for(self.id).is_some() {
            return self.name.clone();
        }
        format!("{}{ACP_DISPLAY_SUFFIX}", self.name)
    }

    #[must_use]
    pub fn terminal_command(&self) -> Option<CommandLine> {
        CommandLine::parse(self.terminal.as_deref()?)
    }

    #[must_use]
    pub fn is_built_in(&self) -> bool {
        is_built_in_acp_agent(self.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Upkeep {
    ByApp,
    ByUser,
}

struct BuiltInAcpAgent {
    id: &'static str,
    name: &'static str,
    program: &'static str,
    args: &'static [&'static str],
    terminal: &'static str,
    package: AcpPackage,
    upkeep: Upkeep,
    stands_for: Option<AgentPreset>,
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

const CLAUDE_AGENT_ACP: NpmPackage = NpmPackage {
    name: "@agentclientprotocol/claude-agent-acp",
    bin: "claude-agent-acp",
};
const CODEX_ACP: NpmPackage = NpmPackage {
    name: "@agentclientprotocol/codex-acp",
    bin: "codex-acp",
};
const GEMINI_CLI: NpmPackage = NpmPackage {
    name: "@google/gemini-cli",
    bin: "gemini",
};
const OPENCODE_INSTALL: &str = "curl -fsSL https://opencode.ai/install | bash";
const CLAUDE_CODE_ACP_ID: &str = "claude-code-acp";

const BUILT_IN_ACP_AGENTS: [BuiltInAcpAgent; 4] = [
    BuiltInAcpAgent {
        id: CLAUDE_CODE_ACP_ID,
        name: "Claude Code",
        program: CLAUDE_AGENT_ACP.bin,
        args: &[],
        terminal: "claude",
        package: AcpPackage::Npm(CLAUDE_AGENT_ACP),
        upkeep: Upkeep::ByApp,
        stands_for: Some(AgentPreset::ClaudeCode),
    },
    BuiltInAcpAgent {
        id: "codex-acp",
        name: "Codex",
        program: CODEX_ACP.bin,
        args: &[],
        terminal: "codex",
        package: AcpPackage::Npm(CODEX_ACP),
        upkeep: Upkeep::ByUser,
        stands_for: None,
    },
    BuiltInAcpAgent {
        id: "opencode",
        name: "opencode",
        program: "opencode",
        args: &["acp"],
        terminal: "opencode",
        package: AcpPackage::Script(OPENCODE_INSTALL),
        upkeep: Upkeep::ByUser,
        stands_for: None,
    },
    BuiltInAcpAgent {
        id: "gemini-cli",
        name: "Gemini CLI",
        program: GEMINI_CLI.bin,
        args: &["--acp"],
        terminal: "gemini",
        package: AcpPackage::Npm(GEMINI_CLI),
        upkeep: Upkeep::ByUser,
        stands_for: None,
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

#[must_use]
pub fn acp_package(id: AcpAgentId) -> Option<AcpPackage> {
    BUILT_IN_ACP_AGENTS
        .iter()
        .find(|built_in| built_in.id == id.as_str())
        .map(|built_in| built_in.package)
}

#[must_use]
pub fn app_kept_acp_package(id: AcpAgentId) -> Option<NpmPackage> {
    BUILT_IN_ACP_AGENTS
        .iter()
        .filter(|built_in| built_in.upkeep == Upkeep::ByApp)
        .find(|built_in| built_in.id == id.as_str())
        .and_then(|built_in| built_in.package.npm())
}

#[must_use]
pub fn is_claude_code_acp(id: AcpAgentId) -> bool {
    id.as_str() == CLAUDE_CODE_ACP_ID
}

#[must_use]
pub fn acp_stand_in_for(id: AcpAgentId) -> Option<AgentPreset> {
    BUILT_IN_ACP_AGENTS
        .iter()
        .find(|built_in| built_in.id == id.as_str())
        .and_then(|built_in| built_in.stands_for)
}

#[must_use]
pub fn acp_stand_in_of(native: AgentPreset) -> Option<AgentPreset> {
    BUILT_IN_ACP_AGENTS
        .iter()
        .find(|built_in| built_in.stands_for == Some(native))
        .and_then(|built_in| built_in.id.parse().ok())
        .map(AgentPreset::Acp)
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
                    "claude-agent-acp",
                    vec![],
                    Some("claude"),
                    "Claude Code".to_owned()
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
                (
                    "gemini-cli",
                    "gemini",
                    vec!["--acp"],
                    Some("gemini"),
                    "Gemini CLI (ACP)".to_owned()
                ),
            ]
        );
        assert!(agents.iter().all(AcpAgent::is_built_in));
        let installs: Vec<Option<String>> = agents
            .iter()
            .map(|agent| acp_package(agent.id).map(|package| package.to_string()))
            .collect();
        assert_eq!(
            installs,
            [
                Some("npm i -g @agentclientprotocol/claude-agent-acp".to_owned()),
                Some("npm i -g @agentclientprotocol/codex-acp".to_owned()),
                Some("curl -fsSL https://opencode.ai/install | bash".to_owned()),
                Some("npm i -g @google/gemini-cli".to_owned()),
            ]
        );
    }

    #[test]
    fn only_claude_code_s_adapter_is_kept_by_the_app() {
        let agents = built_in_acp_agents();
        let kept: Vec<(&str, Option<&str>)> = agents
            .iter()
            .map(|agent| {
                (
                    agent.id.as_str(),
                    app_kept_acp_package(agent.id).map(|package| package.name),
                )
            })
            .collect();
        assert_eq!(
            kept,
            [
                (
                    "claude-code-acp",
                    Some("@agentclientprotocol/claude-agent-acp")
                ),
                ("codex-acp", None),
                ("opencode", None),
                ("gemini-cli", None),
            ]
        );
        let claude = agents
            .iter()
            .filter(|agent| is_claude_code_acp(agent.id))
            .count();
        assert_eq!(claude, 1);
    }

    #[test]
    fn claude_code_s_adapter_stands_for_the_native_claude_code_and_no_other_does() {
        let agents = built_in_acp_agents();
        let stand_ins: Vec<(&str, Option<AgentPreset>)> = agents
            .iter()
            .map(|agent| (agent.id.as_str(), acp_stand_in_for(agent.id)))
            .collect();
        assert_eq!(
            stand_ins,
            [
                ("claude-code-acp", Some(AgentPreset::ClaudeCode)),
                ("codex-acp", None),
                ("opencode", None),
                ("gemini-cli", None),
            ]
        );
        let claude = acp_stand_in_of(AgentPreset::ClaudeCode).and_then(AgentPreset::acp_agent);
        assert!(claude.is_some_and(is_claude_code_acp));
        assert_eq!(acp_stand_in_of(AgentPreset::Codex), None);
    }
}
