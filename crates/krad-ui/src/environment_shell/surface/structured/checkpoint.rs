use krad_domain::{
    ChatCheckpoints, EnvironmentName, GitObjectId, NodeField, RewindPoint, TerminalId,
};
use krad_host::HostLink;
use std::path::PathBuf;

use super::super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn group_folder(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<(HostLink, PathBuf)> {
        let root = self.envs.get(env)?.data.group_of(id)?.root.clone();
        Some((self.env_link(env), root))
    }

    pub(super) fn record_turn_checkpoint(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        point: RewindPoint,
        checkpoint: GitObjectId,
    ) {
        let Some(mut checkpoints) = self.chat_checkpoints(env, id) else {
            return;
        };
        checkpoints.record(point, checkpoint);
        self.set_node(env, id, NodeField::Checkpoints(checkpoints));
        self.store();
    }

    pub(in crate::environment_shell::surface) fn turn_checkpoint(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        point: &RewindPoint,
    ) -> Option<GitObjectId> {
        self.envs
            .get(env)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)?
            .checkpoints
            .before(point)
            .cloned()
    }

    fn chat_checkpoints(&self, env: &EnvironmentName, id: TerminalId) -> Option<ChatCheckpoints> {
        self.envs
            .get(env)?
            .data
            .nodes
            .iter()
            .find(|node| node.id == id)
            .map(|node| node.checkpoints.clone())
    }
}
