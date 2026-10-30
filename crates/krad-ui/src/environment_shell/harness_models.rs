use gpui_kit::{AppContext as _, Context};
use krad_chat::{claude_models, codex_models, ModelOffer, Runner};
use krad_domain::AgentPreset;
use std::collections::HashMap;
use std::path::Path;

use super::EnvironmentShell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Listing {
    Claude,
    Codex,
}

impl Listing {
    const fn of(harness: AgentPreset) -> Option<Self> {
        match harness {
            AgentPreset::ClaudeCode => Some(Self::Claude),
            AgentPreset::Codex => Some(Self::Codex),
            AgentPreset::Shell | AgentPreset::Opencode | AgentPreset::Gateway => None,
        }
    }

    fn run(self, runner: &Runner, cwd: &Path) -> Result<Vec<ModelOffer>, String> {
        match self {
            Self::Claude => claude_models(runner, cwd).map_err(|error| error.to_string()),
            Self::Codex => codex_models(runner, cwd).map_err(|error| error.to_string()),
        }
    }
}

#[derive(Default)]
pub(super) struct ListedModels {
    listed: HashMap<AgentPreset, Vec<ModelOffer>>,
    asked: Vec<AgentPreset>,
}

impl ListedModels {
    pub(super) fn of(&self, harness: AgentPreset) -> &[ModelOffer] {
        self.listed.get(&harness).map_or(&[], Vec::as_slice)
    }

    fn keep(&mut self, harness: AgentPreset, models: Vec<ModelOffer>) {
        if models.is_empty() || Listing::of(harness).is_none() {
            return;
        }
        self.listed.insert(harness, models);
    }
}

impl EnvironmentShell {
    pub(super) fn list_harness_models(&mut self, cx: &mut Context<Self>) {
        for harness in AgentPreset::all() {
            if let Some(listing) = Listing::of(harness) {
                self.list_models_of(harness, listing, cx);
            }
        }
    }

    fn list_models_of(&mut self, harness: AgentPreset, listing: Listing, cx: &mut Context<Self>) {
        let installed = self.native_agents.contains(&harness);
        if !installed || self.listed_models.asked.contains(&harness) {
            return;
        }
        let Ok((runner, cwd)) = self.harness_runner(harness, None) else {
            return;
        };
        self.listed_models.asked.push(harness);
        let listed = cx.background_spawn(async move { listing.run(&runner, &cwd) });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            this.update(cx, |shell, cx| {
                match listed {
                    Ok(models) => shell.listed_models.keep(harness, models),
                    Err(error) => shell.toast(&error, cx),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn keep_harness_models(&mut self, harness: AgentPreset, models: &[ModelOffer]) {
        self.listed_models.keep(harness, models.to_vec());
    }
}
