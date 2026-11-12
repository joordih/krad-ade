use gpui_kit::{AppContext as _, Context};
use krad_chat::{acp_agent_env, AcpConfig, BusVars, ModelOffer, Variables};
use krad_domain::{
    AcpAgent, AcpAgentId, AcpAgentStart, AcpModelId, AgentPreset, EnvironmentName, Host, McpSecret,
    McpVariableName, ModelId, TerminalId,
};
use krad_host::HostLink;
use krad_store::find_acp_secret;
use std::collections::HashMap;

use super::super::super::chat::StartError;
use super::super::super::cloud::NodeSite;
use super::super::super::EnvironmentShell;
use super::super::SurfaceChat;
use super::structured_mut;

const SECRETS_UNREAD_TEXT: &str = "The agent's secrets could not be read";

type AcpSecrets = HashMap<McpVariableName, McpSecret>;

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
    let mut secrets = HashMap::new();
    for key in agent.secret_keys() {
        let found =
            find_acp_secret(&key).map_err(|error| format!("{SECRETS_UNREAD_TEXT}: {error}"))?;
        if let Some((secret, _)) = found {
            secrets.insert(key.variable, secret);
        }
    }
    Ok(secrets)
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
        env: &EnvironmentName,
        id: TerminalId,
        agent: AcpAgentId,
        cx: &mut Context<Self>,
    ) -> Result<AcpConfig, StartError> {
        let preset = AgentPreset::Acp(agent);
        let agent = self
            .settings
            .acp
            .agent(agent)
            .ok_or_else(|| StartError::NoProgram(self.settings.acp.harness_name(preset)))?;
        let root = self
            .envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        let started = match self.node_site(env, id) {
            NodeSite::Env => {
                let link = self.env_link(env);
                let started = self.acp_runner(&link, &agent, root.as_deref(), cx);
                self.note_adapter_install(env, id, agent.id);
                started
            }
            NodeSite::Cloud(site) => {
                self.cloud_chat_runner(env, id, &site, preset, agent.program.clone())
            }
            NodeSite::Waiting(_) => Err(StartError::Waiting),
        };
        let (runner, cwd) = started?;
        if !self.acp_secrets_ready(env, id, &agent, cx) {
            return Err(StartError::Waiting);
        }
        let folder = self.chat_folder(env, id);
        let launch = self
            .gateway_mcp(env, id, &folder, cx)
            .ok_or(StartError::Waiting)?;
        let bus_vars = if !runner.in_cloud() && runner.on_ssh() && self.remote_bus(env).is_none() {
            BusVars(Vec::new())
        } else {
            BusVars(self.bus_vars(env, id))
        };
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
            .map(|(env, live)| (env.clone(), live.link.host()))
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
