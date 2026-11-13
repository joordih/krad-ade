use gpui_kit::{AppContext as _, Context};
use krad_chat::acp_offers;
use krad_domain::{
    AcpAgentId, AcpAgentStart, AgentPreset, EnvironmentName, Host, ModelOffer, ToastKind,
};
use krad_host::HostLink;
use krad_protocol::{Command, ListedModels as Listed, ModelsCommand, ModelsOrigin};
use std::collections::HashMap;

use super::chat::StartError;
use super::surface::{read_acp_secrets, AcpSeed};
use super::EnvironmentShell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AcpListing {
    Listing,
    NoModels,
    Failed(String),
}

#[derive(Default)]
pub(super) struct ListedModels {
    acp: HashMap<(Host, AcpAgentId), AcpListing>,
    acp_seeds: HashMap<(Host, AcpAgentId), AcpSeed>,
}

impl ListedModels {
    pub(super) fn acp_listing(&self, host: &Host, agent: AcpAgentId) -> Option<&AcpListing> {
        self.acp.get(&(host.clone(), agent))
    }

    pub(super) fn acp_seed(&self, host: &Host, agent: AcpAgentId) -> Option<&AcpSeed> {
        self.acp_seeds.get(&(host.clone(), agent))
    }
}

#[derive(Default)]
pub(super) struct ModelsMirror(HashMap<(Host, AgentPreset), Vec<ModelOffer>>);

impl ModelsMirror {
    pub(super) fn of(&self, host: &Host, harness: AgentPreset) -> &[ModelOffer] {
        self.0
            .get(&(host.clone(), harness))
            .map_or(&[], Vec::as_slice)
    }

    fn apply(&mut self, listed: Listed) {
        self.0.insert((listed.host, listed.harness), listed.models);
    }

    pub(super) fn replace(&mut self, listed: Vec<Listed>) {
        self.0.clear();
        for listed in listed {
            self.apply(listed);
        }
    }
}

impl EnvironmentShell {
    pub(super) fn list_harness_models(&self) {
        for harness in AgentPreset::NATIVE {
            if self.native_agents.contains(&harness) {
                self.list_models_of(&HostLink::Local, harness);
            }
        }
    }

    pub(super) fn list_host_models(&self, env: &EnvironmentName) {
        let link = self.env_link(env);
        let Some(found) = self.host_agents.get(&link.host()).cloned() else {
            return;
        };
        for harness in found {
            self.list_models_of(&link, harness);
        }
    }

    fn list_models_of(&self, link: &HostLink, harness: AgentPreset) {
        let Ok((runner, cwd)) = self.harness_runner(link, harness, None) else {
            return;
        };
        self.runtime
            .handle
            .list_models(link.host(), harness, runner, cwd);
    }

    pub(super) fn models_listed(
        &mut self,
        listed: Listed,
        origin: ModelsOrigin,
        cx: &mut Context<Self>,
    ) {
        let harness = listed.harness;
        self.runtime.view.models.apply(listed);
        if origin == ModelsOrigin::Listing {
            self.forget_unoffered_models(harness);
        }
        cx.notify();
    }

    pub(super) fn model_list_failed(&mut self, error: &str, cx: &mut Context<Self>) {
        self.toast(ToastKind::Error, error, cx);
    }

    fn keep_models(&self, host: Host, harness: AgentPreset, models: Vec<ModelOffer>) {
        self.runtime
            .client
            .post(Command::Models(ModelsCommand::Keep {
                host,
                harness,
                models,
            }));
    }

    pub(super) fn list_acp_models(&mut self, agent: AcpAgentId, cx: &mut Context<Self>) {
        let link = self.active_link();
        let host = link.host();
        let preset = AgentPreset::Acp(agent);
        let key = (host.clone(), agent);
        let busy_or_known = matches!(
            self.listed_models.acp.get(&key),
            Some(AcpListing::Listing | AcpListing::NoModels)
        ) || (!self.runtime.view.models.of(&host, preset).is_empty()
            && self.listed_models.acp_seeds.contains_key(&key));
        if busy_or_known || !self.installed_agents().contains(&preset) {
            return;
        }
        let Some(found) = self.settings.acp.agent(agent) else {
            return;
        };
        let root = self.active_group().map(|group| group.root.clone());
        let started = self.acp_runner(&link, &found, root.as_deref());
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
                    Ok(models) => {
                        shell.listed_models.acp.remove(&key);
                        shell.keep_models(host, preset, models);
                    }
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
}
