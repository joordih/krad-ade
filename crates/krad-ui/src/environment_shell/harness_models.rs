use gpui_kit::{AppContext as _, Context};
use krad_chat::{acp_offers, claude_models, codex_models, ModelOffer, Runner};
use krad_domain::{AcpAgentId, AcpAgentStart, AgentPreset, EnvironmentName, Host, ToastKind};
use krad_host::HostLink;
use std::collections::HashMap;
use std::path::Path;

use super::chat::StartError;
use super::surface::{read_acp_secrets, AcpSeed};
use super::EnvironmentShell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AcpListing {
    Listing,
    NoModels,
    Failed(String),
}

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
            AgentPreset::Shell
            | AgentPreset::Opencode
            | AgentPreset::Gateway
            | AgentPreset::Acp(_) => None,
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
    acp: HashMap<(Host, AcpAgentId), AcpListing>,
    acp_seeds: HashMap<(Host, AcpAgentId), AcpSeed>,
}

const fn keeps(harness: AgentPreset) -> bool {
    matches!(harness, AgentPreset::Acp(_)) || Listing::of(harness).is_some()
}

impl ListedModels {
    pub(super) fn of(&self, host: &Host, harness: AgentPreset) -> &[ModelOffer] {
        self.listed
            .get(&(host.clone(), harness))
            .map_or(&[], Vec::as_slice)
    }

    fn keep(&mut self, host: Host, harness: AgentPreset, models: Vec<ModelOffer>) {
        if models.is_empty() || !keeps(harness) {
            return;
        }
        if let AgentPreset::Acp(agent) = harness {
            self.acp.remove(&(host.clone(), agent));
        }
        self.listed.insert((host, harness), models);
    }

    pub(super) fn acp_listing(&self, host: &Host, agent: AcpAgentId) -> Option<&AcpListing> {
        self.acp.get(&(host.clone(), agent))
    }

    pub(super) fn acp_seed(&self, host: &Host, agent: AcpAgentId) -> Option<&AcpSeed> {
        self.acp_seeds.get(&(host.clone(), agent))
    }
}

impl EnvironmentShell {
    pub(super) fn list_harness_models(&mut self, cx: &mut Context<Self>) {
        for harness in AgentPreset::NATIVE {
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
                    Ok(models) => {
                        shell.listed_models.keep(host, harness, models);
                        shell.forget_unoffered_models(harness);
                    }
                    Err(error) => shell.toast(ToastKind::Error, &error, cx),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn list_acp_models(&mut self, agent: AcpAgentId, cx: &mut Context<Self>) {
        let link = self.active_link();
        let host = link.host();
        let preset = AgentPreset::Acp(agent);
        let key = (host.clone(), agent);
        let busy_or_known = matches!(
            self.listed_models.acp.get(&key),
            Some(AcpListing::Listing | AcpListing::NoModels)
        ) || (!self.listed_models.of(&host, preset).is_empty()
            && self.listed_models.acp_seeds.contains_key(&key));
        if busy_or_known || !self.installed_agents().contains(&preset) {
            return;
        }
        let Some(found) = self.settings.acp.agent(agent) else {
            return;
        };
        let root = self.active_group().map(|group| group.root.clone());
        let started = self.acp_runner(&link, &found, root.as_deref(), cx);
        let (runner, cwd) = match started {
            Ok(started) => started,
            Err(StartError::Waiting) => return,
            Err(error) => {
                self.listed_models
                    .acp
                    .insert(key, AcpListing::Failed(error.to_string()));
                return cx.notify();
            }
        };
        self.listed_models
            .acp
            .insert(key.clone(), AcpListing::Listing);
        let listed = cx.background_spawn(async move {
            let secrets = read_acp_secrets(&found)?;
            acp_offers(
                &runner,
                &cwd,
                AcpAgentStart {
                    agent: found,
                    secrets,
                },
            )
            .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            this.update(cx, |shell, cx| {
                let listed = listed.map(|offered| {
                    let seed = AcpSeed::from(&offered);
                    shell.listed_models.acp_seeds.insert(key.clone(), seed);
                    offered.models
                });
                match listed {
                    Ok(models) if models.is_empty() => {
                        shell.listed_models.acp.insert(key, AcpListing::NoModels);
                    }
                    Ok(models) => shell.listed_models.keep(host, preset, models),
                    Err(error) => {
                        shell
                            .listed_models
                            .acp
                            .insert(key, AcpListing::Failed(error));
                    }
                }
                shell.seed_acp_chats();
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
