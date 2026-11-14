use gpui_kit::Context;
use krad_domain::{AcpAgentId, AgentPreset, EnvironmentName, Host, ModelOffer, ToastKind};
use krad_protocol::{
    AcpListing as Listing, AcpOffers, Command, HarnessSpot, ListedModels as Listed, ModelsCommand,
};
use std::collections::HashMap;

use super::surface::AcpSeed;
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

    pub(super) fn apply(&mut self, listed: Listed) {
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
                self.list_models_at(HarnessSpot::default(), harness);
            }
        }
    }

    pub(super) fn list_host_models(&self, env: &EnvironmentName) {
        let host = self.env_host(env);
        let Some(found) = self.host_agents.get(&host).cloned() else {
            return;
        };
        for harness in found {
            self.list_models_at(HarnessSpot::at(env.clone(), None), harness);
        }
    }

    fn list_models_at(&self, spot: HarnessSpot, harness: AgentPreset) {
        self.runtime
            .client
            .post(Command::Models(ModelsCommand::List { spot, harness }));
    }

    pub(super) fn model_list_failed(&mut self, error: &str, cx: &mut Context<Self>) {
        self.toast(ToastKind::Error, error, cx);
    }

    pub(super) fn list_acp_models(&mut self, agent: AcpAgentId, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let host = self.env_host(&env);
        let preset = AgentPreset::Acp(agent);
        let key = (host.clone(), agent);
        let busy_or_known = matches!(
            self.listed_models.acp.get(&key),
            Some(AcpListing::Listing | AcpListing::NoModels)
        ) || (!self
            .runtime
            .view
            .read(cx)
            .models
            .of(&host, preset)
            .is_empty()
            && self.listed_models.acp_seeds.contains_key(&key));
        if busy_or_known
            || !self.installed_agents().contains(&preset)
            || self.settings.acp.agent(agent).is_none()
        {
            return;
        }
        let root = self.active_group().map(|group| group.root.clone());
        self.list_models_at(HarnessSpot::at(env, root), preset);
    }

    pub(super) fn acp_offers_heard(&mut self, offers: AcpOffers, cx: &mut Context<Self>) {
        let key = (offers.host, offers.agent);
        match offers.listing {
            Listing::Listing => {
                self.listed_models.acp.insert(key, AcpListing::Listing);
            }
            Listing::Waiting | Listing::Unknown => {
                self.listed_models.acp.remove(&key);
            }
            Listing::Failed { error } => {
                self.listed_models
                    .acp
                    .insert(key, AcpListing::Failed(error));
            }
            Listing::Offered { offered } => {
                self.listed_models
                    .acp_seeds
                    .insert(key.clone(), AcpSeed::from(&offered));
                if offered.models.is_empty() {
                    self.listed_models.acp.insert(key, AcpListing::NoModels);
                } else {
                    self.listed_models.acp.remove(&key);
                }
                self.seed_acp_chats();
            }
        }
        cx.notify();
    }
}
