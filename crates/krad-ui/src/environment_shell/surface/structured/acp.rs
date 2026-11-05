use gpui_kit::{AppContext as _, Context};
use krad_chat::{AcpConfig, BusVars};
use krad_domain::{
    AcpAgent, AcpAgentId, AcpAgentStart, AgentPreset, EnvironmentName, McpSecret, McpVariableName,
    TerminalId,
};
use krad_store::find_acp_secret;
use std::collections::HashMap;

use super::super::super::chat::StartError;
use super::super::super::EnvironmentShell;
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

fn read_secrets(agent: &AcpAgent) -> Result<AcpSecrets, String> {
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
            .ok_or(StartError::NoProgram(preset))?;
        let root = self
            .envs
            .get(env)
            .and_then(|live| live.data.group_of(id))
            .map(|group| group.root.clone());
        let link = self.env_link(env);
        let (runner, cwd) =
            self.program_runner(&link, preset, agent.program.clone(), root.as_deref())?;
        if !self.acp_secrets_ready(env, id, &agent, cx) {
            return Err(StartError::Waiting);
        }
        let folder = self.chat_folder(env, id);
        let launch = self
            .gateway_mcp(env, id, &folder, cx)
            .ok_or(StartError::Waiting)?;
        let bus_vars = if runner.on_ssh() && self.remote_bus(env).is_none() {
            BusVars(Vec::new())
        } else {
            BusVars(self.bus_vars(env, id))
        };
        let buffers = Self::tab_buffers(env, cx);
        let structured = structured_mut(self, env, id).ok_or(StartError::Waiting)?;
        let secrets = match std::mem::take(&mut structured.acp_secrets) {
            AcpSecretsPrep::Ready(secrets) => secrets.map_err(StartError::Secret)?,
            AcpSecretsPrep::Unread | AcpSecretsPrep::Reading => return Err(StartError::Waiting),
        };
        structured.notices.extend(launch.problems);
        Ok(AcpConfig {
            runner,
            agent: AcpAgentStart { agent, secrets },
            cwd,
            mcp: launch.starts,
            bus_vars,
            buffers,
        })
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
        let reading = cx.background_spawn(async move { read_secrets(&agent) });
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
