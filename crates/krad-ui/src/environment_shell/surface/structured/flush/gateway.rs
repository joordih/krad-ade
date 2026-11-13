use gpui_kit::Context;
use krad_chat::{BusVars, EngineConfig};
use krad_domain::{EnvironmentName, TerminalId};
use krad_host::HostLink;
use krad_store::gateway_sessions_dir;
use std::path::{Path, PathBuf};

use super::super::super::super::chat::StartError;
use super::super::super::super::terminal_card::home_dir;
use super::super::super::super::EnvironmentShell;
use super::super::structured_mut;

impl EnvironmentShell {
    pub(super) fn gateway_config(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> Result<EngineConfig, StartError> {
        if !self.env_link(env).is_ready() {
            return Err(StartError::Waiting);
        }
        if self.gateway_checking() {
            return Err(StartError::Waiting);
        }
        let sessions = gateway_sessions_dir().ok_or(StartError::NoConfigFolder)?;
        let cwd = self.chat_folder(env, id);
        let launch = self
            .gateway_mcp(env, id, &cwd, cx)
            .ok_or(StartError::Waiting)?;
        let host = self.env_link(env);
        let (system_prompt, bus_vars, home) = match &host {
            HostLink::Local => (
                self.system_prompt(env, id, false),
                BusVars(self.bus_vars(env, id)),
                home_dir().map(PathBuf::from),
            ),
            HostLink::Remote(ssh) => {
                let home = ssh.home().map(Path::to_path_buf);
                if self.remote_bus(env).is_some() {
                    (
                        self.system_prompt(env, id, false),
                        BusVars(self.bus_vars(env, id)),
                        home,
                    )
                } else {
                    (self.memory_preamble(env, id), BusVars(Vec::new()), home)
                }
            }
        };
        let chosen = structured_mut(self, env, id).and_then(|chat| chat.chosen_model.clone());
        let model = self.gateway_model(chosen.as_ref());
        if model.is_none() && self.gateway_ready() {
            return Err(StartError::Gateway(
                Self::no_gateway_model_text().to_owned(),
            ));
        }
        let served = self.gateway_served().to_vec();
        let context_windows = self.settings.gateway.context_windows.clone();
        let structured = structured_mut(self, env, id).ok_or(StartError::Waiting)?;
        structured.notices.extend(launch.problems);
        Ok(EngineConfig {
            sessions,
            home,
            cwd,
            model,
            models: served,
            context_windows,
            system_prompt,
            effort: structured.effort,
            permission_mode: structured.permission_mode,
            bus_vars,
            mcp: launch.starts,
            host,
            skills: self.settings.skills.clone(),
        })
    }
}
