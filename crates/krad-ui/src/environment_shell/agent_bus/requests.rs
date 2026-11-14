use krad_domain::agents_api::{ErrorCode, Response, TerminalHandle};
use krad_domain::AgentInfo;
use krad_domain::EnvironmentName;
use krad_runtime::{closed_worker_infos, BusReply, Mail};

use super::super::EnvironmentShell;
use super::feed::lists_as_agent;
use super::NO_SUCH_TERMINAL;

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

    pub(super) fn post(&self, from: &TerminalHandle, to: u64, body: String, reply: BusReply) {
        let env = EnvironmentName::new(from.environment.clone());
        let nodes = self.envs.get(&env).map(|live| &live.data.nodes);
        let Some(nodes) = nodes.filter(|nodes| nodes.iter().any(|node| node.id.as_u64() == to))
        else {
            reply
                .send(Response::error(ErrorCode::NotFound, NO_SUCH_TERMINAL))
                .unwrap_or_default();
            return;
        };
        let from_title = nodes
            .iter()
            .find(|node| node.id.as_u64() == from.terminal)
            .map_or_default(|node| node.title.clone());
        let mail = Mail {
            from: from.clone(),
            from_title,
            to,
            body,
        };
        reply.post_mail(mail).unwrap_or_default();
    }
}
