use gpui_kit::App;
use krad_domain::{EnvironmentName, Host};

use super::super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn seed_acp_chats(&mut self, cx: &App) {
        let hosts: Vec<(EnvironmentName, Host)> = self
            .envs
            .iter()
            .map(|(env, live)| (env.clone(), live.data.host.clone()))
            .collect();
        let model = self.runtime.view.read(cx);
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
                    .and_then(|agent| model.acp.acp_seed(&host, agent));
                if let Some(seed) = seed {
                    structured.acp.seed(seed);
                }
            }
        }
    }
}
