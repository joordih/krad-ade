use krad_protocol::{BusSnapshot, Scope, Seq, Snapshot};

use crate::attention::Attention;
use crate::bus::Bus;
use crate::chats::Chats;
use crate::environments::Environments;
use crate::gateway::Gateway;
use crate::git::Git;
use crate::harness_health::Health;
use crate::hosts::Hosts;
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
    pub(super) opencode: &'a OpencodeServers,
    pub(super) gateway: &'a Gateway,
    pub(super) settings: &'a AppSettings,
    pub(super) git: &'a Git,
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
            hosts: self.hosts.snapshot(),
            chats: self.chats.snapshot(),
            opencode: self.opencode.snapshot(),
            gateway: self.gateway.snapshot(),
            settings: self.settings.snapshot(),
            git: self.git.snapshot(),
        }
    }
}
