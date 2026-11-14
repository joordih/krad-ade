use krad_domain::AgentInfo;
use krad_domain::EnvironmentName;
use krad_runtime::closed_worker_infos;

use super::super::EnvironmentShell;
use super::feed::lists_as_agent;

pub(super) const CLOSED_TERMINAL: &str = "That terminal has no running session";

impl EnvironmentShell {
    pub(super) fn agent_infos(&self, environment: &str, me: Option<u64>) -> Vec<AgentInfo> {
        let env = EnvironmentName::new(environment.to_owned());
        let Some(live) = self.envs.get(&env) else {
            return Vec::new();
        };
        live.data
            .nodes
            .iter()
            .filter(|node| lists_as_agent(node))
            .map(|node| AgentInfo {
                is_self: Some(node.id.as_u64()) == me,
                ..self.agent_info(&env, live, node)
            })
            .collect()
    }

    pub(super) fn all_agent_infos(&self, environment: &str, me: Option<u64>) -> Vec<AgentInfo> {
        let mut agents = self.agent_infos(environment, me);
        if let Some(live) = self.envs.get(&EnvironmentName::new(environment.to_owned())) {
            agents.extend(closed_worker_infos(&live.data));
        }
        agents
    }
}
