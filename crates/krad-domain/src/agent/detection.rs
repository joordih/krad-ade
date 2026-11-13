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
}
