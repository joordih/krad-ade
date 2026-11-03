use gpui_kit::{AppContext as _, Context};
use krad_chat::{claude_models, codex_models, ModelOffer, Runner};
use krad_domain::{AgentPreset, EnvironmentName, Host};
use krad_host::HostLink;
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
    listed: HashMap<(Host, AgentPreset), Vec<ModelOffer>>,
    asked: Vec<(Host, AgentPreset)>,
}

impl ListedModels {
    pub(super) fn of(&self, host: &Host, harness: AgentPreset) -> &[ModelOffer] {
        self.listed
            .get(&(host.clone(), harness))
            .map_or(&[], Vec::as_slice)
    }

    fn keep(&mut self, host: Host, harness: AgentPreset, models: Vec<ModelOffer>) {
        if models.is_empty() || Listing::of(harness).is_none() {
            return;
        }
        self.listed.insert((host, harness), models);
    }
}

impl EnvironmentShell {
    pub(super) fn list_harness_models(&mut self, cx: &mut Context<Self>) {
        for harness in AgentPreset::all() {
            if let Some(listing) = Listing::of(harness) {
                if self.native_agents.contains(&harness) {
                    self.list_models_of(&HostLink::Local, harness, listing, cx);
                }
            }
        }
    }

    pub(super) fn list_host_models(&mut self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let link = self.env_link(env);
        let Some(found) = self.host_agents.get(&link.host()).cloned() else {
            return;
        };
        for harness in found {
            if let Some(listing) = Listing::of(harness) {
                self.list_models_of(&link, harness, listing, cx);
            }
        }
    }

    fn list_models_of(
        &mut self,
        link: &HostLink,
        harness: AgentPreset,
        listing: Listing,
        cx: &mut Context<Self>,
    ) {
        let host = link.host();
        let asked = (host.clone(), harness);
        if self.listed_models.asked.contains(&asked) {
            return;
        }
        let Ok((runner, cwd)) = self.harness_runner(link, harness, None) else {
            return;
        };
        self.listed_models.asked.push(asked);
        let listed = cx.background_spawn(async move { listing.run(&runner, &cwd) });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            this.update(cx, |shell, cx| {
                match listed {
                    Ok(models) => shell.listed_models.keep(host, harness, models),
                    Err(error) => shell.toast(&error, cx),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn keep_harness_models(
        &mut self,
        env: &EnvironmentName,
        harness: AgentPreset,
        models: &[ModelOffer],
    ) {
        let host = self.env_link(env).host();
        self.listed_models.keep(host, harness, models.to_vec());
    }
}
