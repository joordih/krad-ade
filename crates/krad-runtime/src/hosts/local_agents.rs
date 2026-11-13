use krad_chat::managed_adapter_program;
use krad_domain::{agent_programs, app_kept_acp_package, presets_found, AcpAgent, AgentPreset};
use krad_protocol::{AgentPlace, EventKind, LocalAgents};
use krad_repo::wsl_programs;
use krad_session::find_program;
use krad_store::acp_adapters_dir;

use super::Hosts;
use crate::jobs::{JobDone, Jobs};

const NPM_PROGRAM: &str = "npm";

#[derive(Debug)]
pub(crate) struct LocalFound(LocalAgents);

fn adapter_present(agent: &AcpAgent) -> bool {
    let Some(package) = app_kept_acp_package(agent.id) else {
        return false;
    };
    let kept = acp_adapters_dir()
        .map(|dir| dir.join(agent.id.as_str()))
        .and_then(|folder| managed_adapter_program(&folder, package));
    kept.is_some() || find_program(NPM_PROGRAM).is_some()
}

fn presets_here(acp_agents: &[AcpAgent]) -> Vec<AgentPreset> {
    let mut found = presets_found(acp_agents, |program| find_program(program).is_some());
    let kept: Vec<AgentPreset> = acp_agents
        .iter()
        .filter(|agent| adapter_present(agent))
        .map(|agent| AgentPreset::Acp(agent.id))
        .filter(|preset| !found.contains(preset))
        .collect();
    found.extend(kept);
    found
}

fn presets_in_wsl(distro: &str, acp_agents: &[AcpAgent]) -> Vec<AgentPreset> {
    let found = wsl_programs(distro, &agent_programs(acp_agents)).unwrap_or_else(|error| {
        tracing::debug!(%error, "could not look for agent CLIs in WSL");
        Vec::new()
    });
    presets_found(acp_agents, |program| found.contains(&program))
}

impl Hosts {
    pub(crate) fn detect_local_agents(place: AgentPlace, acp_agents: Vec<AcpAgent>, jobs: &Jobs) {
        let started = jobs.run(move || {
            let agents = match &place {
                AgentPlace::Native => presets_here(&acp_agents),
                AgentPlace::Wsl { distro } => presets_in_wsl(distro, &acp_agents),
            };
            JobDone::LocalAgentsFound(LocalFound(LocalAgents { place, agents }))
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not start looking for agent CLIs on this computer");
        }
    }

    pub(crate) fn local_agents_found(&mut self, found: LocalFound, outbox: &mut Vec<EventKind>) {
        let LocalFound(found) = found;
        self.local.insert(found.place.clone(), found.agents.clone());
        outbox.push(EventKind::LocalAgentsFound(found));
    }

    pub(crate) fn local_agents(&self, place: &AgentPlace) -> Option<&[AgentPreset]> {
        self.local.get(place).map(Vec::as_slice)
    }

    pub(super) fn local_snapshot(&self) -> Vec<LocalAgents> {
        self.local
            .iter()
            .map(|(place, agents)| LocalAgents {
                place: place.clone(),
                agents: agents.clone(),
            })
            .collect()
    }
}
