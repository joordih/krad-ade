use gpui_kit::{AppContext as _, Context};
use krad_chat::{acp_agent_env, AcpConfig, BusVars, ModelOffer, Variables};
use krad_domain::{
    AcpAgent, AcpAgentId, AcpAgentStart, AcpModelId, AgentPreset, EnvironmentName, Host, ModelId,
    TerminalId,
};
use krad_host::HostLink;
use krad_store::AcpSecrets;

use super::super::super::chat::StartError;
use super::super::super::cloud::CloudSite;
use super::super::super::EnvironmentShell;
use super::super::SurfaceChat;
use super::structured_mut;

#[derive(Debug, Default)]
pub(in crate::environment_shell) enum AcpSecretsPrep {
    #[default]
    Unread,
    Reading,
    Ready(Result<AcpSecrets, String>),
}

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

fn acp_lists(offers: Option<&[ModelOffer]>, model: &ModelId) -> bool {
    offers.is_none_or(|offers| offers.iter().any(|offer| offer.answers_to(model.as_str())))
}

impl EnvironmentShell {
    pub(super) fn acp_config(
        &mut self,
        (env, id): (&EnvironmentName, TerminalId),
        agent: AcpAgentId,
        site: &CloudSite,
        cx: &mut Context<Self>,
    ) -> Result<AcpConfig, StartError> {
        let preset = AgentPreset::Acp(agent);
        let agent = self
            .settings
            .acp
            .agent(agent)
            .ok_or_else(|| StartError::NoProgram(self.settings.acp.harness_name(preset)))?;
        let (runner, cwd) = self.cloud_chat_runner(env, id, site, preset, agent.program.clone())?;
        if !self.acp_secrets_ready(env, id, &agent, cx) {
            return Err(StartError::Waiting);
        }
        let folder = self.chat_folder(env, id);
        let launch = self
            .gateway_mcp(env, id, &folder, cx)
            .ok_or(StartError::Waiting)?;
        let bus_vars = BusVars(self.bus_vars(env, id));
        let buffers = Self::tab_buffers(env, cx);
        let model = self.acp_model(env, id, preset);
        let structured = structured_mut(self, env, id).ok_or(StartError::Waiting)?;
        let secrets = match std::mem::take(&mut structured.acp_secrets) {
            AcpSecretsPrep::Ready(secrets) => secrets.map_err(StartError::Secret)?,
            AcpSecretsPrep::Unread | AcpSecretsPrep::Reading => return Err(StartError::Waiting),
        };
        structured.notices.extend(launch.problems);
        let wanted = structured.acp.wanted();
        Ok(AcpConfig {
            runner,
            agent: AcpAgentStart { agent, secrets },
            cwd,
            mcp: launch.starts,
            bus_vars,
            buffers,
            model,
            wanted,
        })
    }

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
                .filter_map(SurfaceChat::structured_mut)
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

    fn acp_model(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        preset: AgentPreset,
    ) -> Option<AcpModelId> {
        let chosen = structured_mut(self, env, id)?.chosen_model.clone()?;
        acp_lists(self.harness_offers(preset), &chosen)
            .then(|| AcpModelId(chosen.as_str().to_owned()))
    }

    fn acp_secrets_ready(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        agent: &AcpAgent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(structured) = structured_mut(self, env, id) else {
            return false;
        };
        match structured.acp_secrets {
            AcpSecretsPrep::Ready(_) => return true,
            AcpSecretsPrep::Reading => return false,
            AcpSecretsPrep::Unread => structured.acp_secrets = AcpSecretsPrep::Reading,
        }
        let agent = agent.clone();
        let reading = cx.background_spawn(async move { read_acp_secrets(&agent) });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let secrets = reading.await;
            this.update(cx, |shell, cx| {
                if let Some(structured) = structured_mut(shell, &env, id) {
                    structured.acp_secrets = AcpSecretsPrep::Ready(secrets);
                }
                shell.flush_structured(&env, id, cx);
            })
            .unwrap_or_default();
        })
        .detach();
        false
    }
}
