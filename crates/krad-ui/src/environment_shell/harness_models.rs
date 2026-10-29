use gpui_kit::{AppContext as _, Context};
use krad_chat::{claude_models, codex_models, ModelOffer};
use krad_domain::AgentPreset;

use super::EnvironmentShell;

const LISTED_HARNESSES: [AgentPreset; 2] = [AgentPreset::ClaudeCode, AgentPreset::Codex];

#[derive(Default)]
pub(super) struct HarnessModels {
    claude: Vec<ModelOffer>,
    codex: Vec<ModelOffer>,
    asked: Vec<AgentPreset>,
}

impl HarnessModels {
    pub(super) fn of(&self, harness: AgentPreset) -> &[ModelOffer] {
        match harness {
            AgentPreset::ClaudeCode => &self.claude,
            AgentPreset::Codex => &self.codex,
            _ => &[],
        }
    }

    fn keep(&mut self, harness: AgentPreset, models: Vec<ModelOffer>) {
        if models.is_empty() {
            return;
        }
        match harness {
            AgentPreset::ClaudeCode => self.claude = models,
            AgentPreset::Codex => self.codex = models,
            _ => {}
        }
    }
}

impl EnvironmentShell {
    pub(super) fn list_harness_models(&mut self, cx: &mut Context<Self>) {
        for harness in LISTED_HARNESSES {
            self.list_models_of(harness, cx);
        }
    }

    fn list_models_of(&mut self, harness: AgentPreset, cx: &mut Context<Self>) {
        let installed = self.native_agents.contains(&harness);
        if !installed || self.harness_models.asked.contains(&harness) {
            return;
        }
        let Ok((runner, cwd)) = self.harness_runner(harness, None) else {
            return;
        };
        self.harness_models.asked.push(harness);
        let listed = cx.background_spawn(async move {
            match harness {
                AgentPreset::Codex => {
                    codex_models(&runner, &cwd).map_err(|error| error.to_string())
                }
                _ => claude_models(&runner, &cwd).map_err(|error| error.to_string()),
            }
        });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            this.update(cx, |shell, cx| {
                match listed {
                    Ok(models) => shell.harness_models.keep(harness, models),
                    Err(error) => shell.toast(&error, cx),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn keep_harness_models(&mut self, harness: AgentPreset, models: &[ModelOffer]) {
        self.harness_models.keep(harness, models.to_vec());
    }
}
