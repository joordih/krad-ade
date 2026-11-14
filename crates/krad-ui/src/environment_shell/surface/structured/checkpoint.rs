use gpui_kit::App;
use krad_domain::{
    ChatCheckpoints, EnvironmentName, GitObjectId, NodeField, RewindPoint, TerminalId,
};
use std::path::PathBuf;

use super::super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn group_root(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<PathBuf> {
        Some(self.envs.get(env)?.data.group_of(id)?.root.clone())
    }

    pub(super) fn record_turn_checkpoint(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        point: RewindPoint,
        checkpoint: GitObjectId,
        cx: &mut App,
    ) {
        let Some(mut checkpoints) = self.chat_checkpoints(env, id) else {
            return;
        };
        checkpoints.record(point, checkpoint);
        self.set_node(env, id, NodeField::Checkpoints(checkpoints), cx);
        self.save_view();
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
