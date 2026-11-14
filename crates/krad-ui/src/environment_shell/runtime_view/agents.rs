use krad_domain::{AcpAgent, AgentPreset, EnvironmentData, Host};
use krad_protocol::{AgentPlace, EnvironmentHost, LocalAgents};
use std::collections::HashMap;

const SHELL_ONLY: &[AgentPreset] = &[AgentPreset::Shell];

pub(in crate::environment_shell) struct AgentsMirror {
    native: Vec<AgentPreset>,
    wsl: HashMap<String, Vec<AgentPreset>>,
    hosts: HashMap<Host, Vec<AgentPreset>>,
    asked_acp: Option<Vec<AcpAgent>>,
}

impl Default for AgentsMirror {
    fn default() -> Self {
        Self {
            native: SHELL_ONLY.to_vec(),
            wsl: HashMap::new(),
            hosts: HashMap::new(),
            asked_acp: None,
        }
    }
}

impl AgentsMirror {
    pub(super) fn found_local(&mut self, found: &LocalAgents) {
        match &found.place {
            AgentPlace::Wsl { distro } => {
                self.wsl.insert(distro.clone(), found.agents.clone());
            }
            AgentPlace::Native => self.native.clone_from(&found.agents),
        }
    }

    pub(super) fn found_on(&mut self, host: Host, agents: Vec<AgentPreset>) {
        self.hosts.insert(host, agents);
    }

    pub(super) fn replace(
        &mut self,
        hosts: &[EnvironmentHost],
        environments: &[EnvironmentData],
        local: &[LocalAgents],
    ) {
        for entry in hosts.iter().filter(|entry| !entry.agents.is_empty()) {
            if let Some(data) = environments
                .iter()
                .find(|data| data.name == entry.environment)
            {
                self.found_on(data.host.clone(), entry.agents.clone());
            }
        }
        for found in local {
            self.found_local(found);
        }
    }

    pub(in crate::environment_shell) fn native(&self) -> &[AgentPreset] {
        &self.native
    }

    pub(in crate::environment_shell) fn on_host(&self, host: &Host) -> Option<&[AgentPreset]> {
        self.hosts.get(host).map(Vec::as_slice)
    }

    pub(in crate::environment_shell) fn on_host_or_shell(&self, host: &Host) -> &[AgentPreset] {
        self.on_host(host).unwrap_or(SHELL_ONLY)
    }

    pub(in crate::environment_shell) fn in_wsl(&self, distro: &str) -> Option<&[AgentPreset]> {
        self.wsl.get(distro).map(Vec::as_slice)
    }

    pub(in crate::environment_shell) fn ask_in_wsl(&mut self, distro: &str) -> bool {
        if self.wsl.contains_key(distro) {
            return false;
        }
        self.wsl.insert(distro.to_owned(), SHELL_ONLY.to_vec());
        true
    }

    pub(in crate::environment_shell) fn wsl_distros(&self) -> Vec<String> {
        self.wsl.keys().cloned().collect()
    }

    pub(in crate::environment_shell) fn acp_asked(&mut self, agents: Vec<AcpAgent>) -> bool {
        if self.asked_acp.as_ref() == Some(&agents) {
            return false;
        }
        self.asked_acp = Some(agents);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::EnvironmentName;

    #[test]
    fn agents_are_read_per_place_from_the_runtime_s_findings(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut mirror = AgentsMirror::default();
        assert_eq!(mirror.native(), SHELL_ONLY);
        mirror.found_local(&LocalAgents {
            place: AgentPlace::Native,
            agents: vec![AgentPreset::Shell, AgentPreset::ClaudeCode],
        });
        assert!(mirror.native().contains(&AgentPreset::ClaudeCode));
        assert!(mirror.ask_in_wsl("Ubuntu"));
        assert!(!mirror.ask_in_wsl("Ubuntu"));
        assert_eq!(mirror.in_wsl("Ubuntu"), Some(SHELL_ONLY));
        let host = Host::Ssh {
            destination: "dev@box".parse()?,
        };
        let mut data = EnvironmentData::empty(EnvironmentName::new("remote".to_owned()));
        data.host = host.clone();
        let entry = EnvironmentHost {
            environment: data.name.clone(),
            state: krad_protocol::HostState::default(),
            bus: krad_protocol::RemoteBusState::default(),
            agents: vec![AgentPreset::Codex],
        };
        mirror.replace(&[entry], &[data], &[]);
        assert_eq!(mirror.on_host(&host), Some(&[AgentPreset::Codex][..]));
        assert_eq!(
            mirror.on_host_or_shell(&Host::Ssh {
                destination: "other@box".parse()?,
            }),
            SHELL_ONLY
        );
        Ok(())
    }
}
