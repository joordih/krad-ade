use crate::{AcpAgent, AgentPreset};

#[must_use]
pub fn agent_programs(acp_agents: &[AcpAgent]) -> Vec<&str> {
    let mut programs: Vec<&str> = AgentPreset::NATIVE
        .into_iter()
        .filter_map(AgentPreset::program)
        .collect();
    programs.extend(acp_agents.iter().map(|agent| agent.program.as_str()));
    programs.sort_unstable();
    programs.dedup();
    programs
}

#[must_use]
pub fn presets_found(
    acp_agents: &[AcpAgent],
    is_installed: impl Fn(&str) -> bool,
) -> Vec<AgentPreset> {
    let native = AgentPreset::NATIVE
        .into_iter()
        .filter(|preset| preset.program().is_none_or(&is_installed));
    let acp = acp_agents
        .iter()
        .filter(|agent| is_installed(&agent.program))
        .map(|agent| AgentPreset::Acp(agent.id));
    native.chain(acp).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::built_in_acp_agents;

    #[test]
    fn only_installed_programs_offer_their_presets() {
        let programs = agent_programs(&[]);
        assert!(programs.windows(2).all(|pair| pair[0] < pair[1]));
        let found = presets_found(&[], |_| false);
        assert!(found.contains(&AgentPreset::Shell));
        assert!(!found.contains(&AgentPreset::ClaudeCode));
        let everything = presets_found(&[], |_| true);
        assert!(everything.contains(&AgentPreset::ClaudeCode));
    }

    #[test]
    fn acp_agents_are_listed_once_their_program_is_found() {
        let agents = built_in_acp_agents();
        let found = ["claude", "codex-acp", "opencode"];
        let presets = presets_found(&agents, |program| found.contains(&program));
        let acp: Vec<String> = presets
            .iter()
            .filter_map(|preset| preset.acp_agent())
            .map(|id| id.to_string())
            .collect();
        assert_eq!(acp, ["codex-acp", "opencode"]);
        assert!(presets.contains(&AgentPreset::ClaudeCode));
        assert!(!presets.contains(&AgentPreset::Codex));
        assert_eq!(
            agent_programs(&agents),
            [
                "claude",
                "claude-agent-acp",
                "codex",
                "codex-acp",
                "gemini",
                "opencode"
            ]
        );
    }
}
