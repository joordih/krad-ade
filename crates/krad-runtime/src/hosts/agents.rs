use krad_domain::{agent_programs, presets_found, AcpAgent, AgentPreset, EnvironmentName, Host};
use krad_host::HostLink;
use krad_protocol::EventKind;
use krad_repo::host_programs;

use super::Hosts;
use crate::jobs::{JobDone, Jobs};

#[derive(Debug)]
pub(crate) struct AgentsFound {
    environment: EnvironmentName,
    host: Host,
    agents: Vec<AgentPreset>,
}

fn presets_on_host(link: &HostLink, acp_agents: &[AcpAgent]) -> Vec<AgentPreset> {
    let found = host_programs(link, &agent_programs(acp_agents)).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not look for agent CLIs on the host");
        Vec::new()
    });
    presets_found(acp_agents, |program| found.contains(&program))
}

impl Hosts {
    pub(crate) fn detect_agents(
        &self,
        environment: &EnvironmentName,
        acp_agents: Vec<AcpAgent>,
        jobs: &Jobs,
    ) {
        let Some(entry) = self
            .envs
            .get(environment)
            .filter(|entry| entry.tracked.state.is_connected())
        else {
            return;
        };
        let host = entry.host.clone();
        let link = self.links.link(&host);
        let environment = environment.clone();
        let started = jobs.run(move || {
            let agents = presets_on_host(&link, &acp_agents);
            JobDone::HostAgentsFound(AgentsFound {
                environment,
                host,
                agents,
            })
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not start looking for agent CLIs on the host");
        }
    }

    pub(crate) fn agents_found(&mut self, found: AgentsFound, outbox: &mut Vec<EventKind>) {
        let AgentsFound {
            environment,
            host,
            agents,
        } = found;
        let Some(entry) = self
            .envs
            .get_mut(&environment)
            .filter(|entry| entry.host == host)
        else {
            return;
        };
        entry.agents.clone_from(&agents);
        outbox.push(EventKind::HostAgentsFound {
            environment,
            host,
            agents,
        });
    }
}
