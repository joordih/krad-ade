use super::{built_in_acp_agents, is_built_in_acp_agent, AcpAgent, AcpAgentId};
use crate::agent::AgentPreset;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcpAgentRejected {
    BuiltIn(AcpAgentId),
    Taken(AcpAgentId),
    NoName,
    NoProgram,
}

impl fmt::Display for AcpAgentRejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(id) => write!(formatter, "{id} is the id of a built-in agent"),
            Self::Taken(id) => write!(formatter, "There is already an agent with the id {id}"),
            Self::NoName => formatter.write_str("Enter a name"),
            Self::NoProgram => formatter.write_str("Enter the program to run"),
        }
    }
}

impl std::error::Error for AcpAgentRejected {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpSettings {
    #[serde(
        default,
        deserialize_with = "valid_agents",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub agents: Vec<AcpAgent>,
}

fn valid_agents<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<AcpAgent>, D::Error> {
    let mut settings = AcpSettings::default();
    for agent in Vec::<Value>::deserialize(deserializer)?
        .into_iter()
        .filter_map(|raw| serde_json::from_value::<AcpAgent>(raw).ok())
    {
        if settings.admits(&agent).is_ok() {
            settings.agents.push(agent);
        }
    }
    Ok(settings.agents)
}

impl AcpSettings {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// # Errors
    ///
    /// Fails when the agent reuses a built-in or a kept id, or has no name or
    /// program.
    pub fn add_agent(&mut self, agent: AcpAgent) -> Result<(), AcpAgentRejected> {
        self.admits(&agent)?;
        self.agents.push(agent);
        Ok(())
    }

    fn admits(&self, agent: &AcpAgent) -> Result<(), AcpAgentRejected> {
        if agent.name.trim().is_empty() {
            return Err(AcpAgentRejected::NoName);
        }
        if agent.program.trim().is_empty() {
            return Err(AcpAgentRejected::NoProgram);
        }
        if is_built_in_acp_agent(agent.id) {
            return Err(AcpAgentRejected::BuiltIn(agent.id));
        }
        if self.agents.iter().any(|kept| kept.id == agent.id) {
            return Err(AcpAgentRejected::Taken(agent.id));
        }
        Ok(())
    }

    pub fn remove_agent(&mut self, id: AcpAgentId) -> Option<AcpAgent> {
        let at = self.agents.iter().position(|kept| kept.id == id)?;
        Some(self.agents.remove(at))
    }

    #[must_use]
    pub fn agent(&self, id: AcpAgentId) -> Option<AcpAgent> {
        built_in_acp_agents()
            .into_iter()
            .find(|agent| agent.id == id)
            .or_else(|| self.agents.iter().find(|agent| agent.id == id).cloned())
    }

    #[must_use]
    pub fn harness_name(&self, preset: AgentPreset) -> Cow<'static, str> {
        preset
            .acp_agent()
            .and_then(|id| self.agent(id))
            .map_or(Cow::Borrowed(preset.display_name()), |agent| {
                Cow::Owned(agent.display_name())
            })
    }

    #[must_use]
    pub fn all_agents(&self) -> Vec<AcpAgent> {
        built_in_acp_agents()
            .into_iter()
            .chain(self.agents.iter().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpValue, McpVariable};

    fn agent(id: &str) -> AcpAgent {
        AcpAgent {
            id: id.parse().unwrap_or_else(|_| unreachable!("{id} is an id")),
            name: "Gemini".to_owned(),
            program: "gemini".to_owned(),
            args: vec!["--experimental-acp".to_owned()],
            env: vec![McpVariable {
                name: "GEMINI_API_KEY"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("a variable name")),
                value: McpValue::Secret,
            }],
            terminal: None,
        }
    }

    #[test]
    fn custom_agents_follow_the_built_in_ones_and_never_take_their_ids() {
        let mut settings = AcpSettings::default();
        assert_eq!(settings.add_agent(agent("gemini")), Ok(()));
        assert_eq!(
            settings.add_agent(agent("gemini")),
            Err(AcpAgentRejected::Taken(agent("gemini").id))
        );
        assert_eq!(
            settings.add_agent(agent("codex-acp")),
            Err(AcpAgentRejected::BuiltIn(agent("codex-acp").id))
        );
        let nameless = AcpAgent {
            name: " ".to_owned(),
            ..agent("other")
        };
        assert_eq!(settings.add_agent(nameless), Err(AcpAgentRejected::NoName));
        let ids: Vec<String> = settings
            .all_agents()
            .iter()
            .map(|agent| agent.id.to_string())
            .collect();
        assert_eq!(ids, ["claude-code-acp", "codex-acp", "opencode", "gemini"]);
        let names = [
            AgentPreset::Codex,
            AgentPreset::Acp(agent("gemini").id),
            AgentPreset::Acp(agent("codex-acp").id),
            AgentPreset::Acp(agent("gone").id),
        ]
        .map(|preset| settings.harness_name(preset));
        assert_eq!(names, ["Codex", "Gemini (ACP)", "Codex (ACP)", "ACP agent"]);
        assert_eq!(
            settings.remove_agent(agent("gemini").id),
            Some(agent("gemini"))
        );
        assert!(settings.is_empty());
    }

    #[test]
    fn custom_agents_round_trip_and_unreadable_or_built_in_ones_are_dropped(
    ) -> serde_json::Result<()> {
        let mut settings = AcpSettings::default();
        assert_eq!(settings.add_agent(agent("gemini")), Ok(()));
        let text = serde_json::to_string(&settings)?;
        assert_eq!(
            text,
            r#"{"agents":[{"id":"gemini","name":"Gemini","program":"gemini","args":["--experimental-acp"],"env":[{"name":"GEMINI_API_KEY","value":"secret"}]}]}"#
        );
        assert_eq!(serde_json::from_str::<AcpSettings>(&text)?, settings);
        let read: AcpSettings = serde_json::from_str(
            r#"{"agents":[
                {"id":"Bad Id","name":"x","program":"x"},
                {"id":"opencode","name":"Mine","program":"opencode"},
                {"id":"gemini","name":"Gemini","program":"gemini","args":["--experimental-acp"],"env":[{"name":"GEMINI_API_KEY","value":"secret"}]},
                {"id":"gemini","name":"Twice","program":"gemini"}
            ]}"#,
        )?;
        assert_eq!(read, settings);
        Ok(())
    }
}
