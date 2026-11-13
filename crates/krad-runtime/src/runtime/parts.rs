use krad_protocol::{BusSnapshot, HostsSnapshot, Scope, Seq, Snapshot};

use super::Runtime;
use crate::adapters::AcpAdapters;
use crate::attention::Attention;
use crate::bus::Bus;
use crate::chats::Chats;
use crate::environments::Environments;
use crate::gateway::Gateway;
use crate::git::Git;
use crate::harness_health::Health;
use crate::hosts::Hosts;
use crate::kraddies::Kraddies;
use crate::models::Models;
use crate::opencode::OpencodeServers;
use crate::settings::AppSettings;
use crate::terminals::Terminals;
use crate::usage::Usage;

pub(super) struct Parts<'a> {
    pub(super) bus: Option<&'a Bus>,
    pub(super) attention: &'a Attention,
    pub(super) health: &'a Health,
    pub(super) usage: &'a Usage,
    pub(super) models: &'a Models,
    pub(super) environments: &'a Environments,
    pub(super) terminals: &'a Terminals,
    pub(super) hosts: &'a Hosts,
    pub(super) chats: &'a Chats,
    pub(super) adapters: &'a AcpAdapters,
    pub(super) opencode: &'a OpencodeServers,
    pub(super) gateway: &'a Gateway,
    pub(super) settings: &'a AppSettings,
    pub(super) git: &'a Git,
    pub(super) kraddies: &'a Kraddies,
}

impl Parts<'_> {
    pub(super) fn snapshot(&self, seq: Seq, scope: Scope) -> Snapshot {
        Snapshot {
            scope,
            seq,
            bus: self.bus.map_or_else(BusSnapshot::default, Bus::snapshot),
            attention: self.attention.snapshot(),
            health: self.health.snapshot(),
            usage: self.usage.snapshot(),
            models: self.models.snapshot(),
            environments: self.environments.snapshot(),
            terminals: self.terminals.snapshot(),
            hosts: Box::new(HostsSnapshot {
                adapters: self.adapters.snapshot(),
                ..self.hosts.snapshot()
            }),
            chats: self.chats.snapshot(),
            opencode: self.opencode.snapshot(),
            gateway: self.gateway.snapshot(),
            settings: self.settings.snapshot(),
            git: self.git.snapshot(),
            kraddies: Box::new(self.kraddies.snapshot()),
        }
    }
}

impl Runtime {
    pub(super) fn snapshot(&self, scope: Scope) -> Snapshot {
        Parts {
            bus: self.bus.as_ref(),
            attention: &self.attention,
            health: &self.health,
            usage: &self.usage,
            models: &self.models,
            environments: &self.environments,
            terminals: &self.terminals,
            hosts: &self.hosts,
            chats: &self.chats,
            adapters: &self.adapters,
            opencode: &self.opencode,
            gateway: &self.gateway,
            settings: &self.settings,
            git: &self.git,
            kraddies: &self.kraddies,
        }
        .snapshot(self.seq, scope)
    }
}
