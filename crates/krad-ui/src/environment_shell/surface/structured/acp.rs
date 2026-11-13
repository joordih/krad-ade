use krad_chat::{acp_agent_env, Variables};
use krad_domain::{AcpAgent, AcpAgentStart, EnvironmentName, Host};
use krad_host::HostLink;
use krad_store::AcpSecrets;

use super::super::super::EnvironmentShell;

pub(in crate::environment_shell) fn read_acp_secrets(
    agent: &AcpAgent,
) -> Result<AcpSecrets, String> {
    krad_store::read_acp_secrets(agent).map_err(|error| error.to_string())
}

pub(in crate::environment_shell::surface) fn read_acp_env(
    agent: AcpAgent,
    link: &HostLink,
) -> Result<Vec<(String, String)>, String> {
    let secrets = read_acp_secrets(&agent)?;
    acp_agent_env(&AcpAgentStart { agent, secrets }, Variables::from(link))
        .map_err(|error| error.to_string())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn seed_acp_chats(&mut self) {
        let hosts: Vec<(EnvironmentName, Host)> = self
            .envs
            .iter()
            .map(|(env, live)| (env.clone(), live.data.host.clone()))
            .collect();
        for (env, host) in hosts {
            let Some(live) = self.envs.get_mut(&env) else {
                continue;
            };
            for structured in live
                .chats
                .values_mut()
                .filter_map(|view| view.chat.structured_mut())
            {
                let seed = structured
                    .harness
                    .acp_agent()
                    .and_then(|agent| self.listed_models.acp_seed(&host, agent));
                if let Some(seed) = seed {
                    structured.acp.seed(seed);
                }
            }
        }
    }
}
