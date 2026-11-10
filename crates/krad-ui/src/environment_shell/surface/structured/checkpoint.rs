use gpui_kit::{AppContext as _, Context};
use krad_chat::{ChatEvent, LiveEvent};
use krad_domain::{EnvironmentName, GitObjectId, RewindPoint, TerminalId, TerminalNode};
use krad_host::HostLink;
use krad_repo::{snapshot_checkpoint, RepoError};
use std::collections::VecDeque;
use std::path::PathBuf;

use super::super::super::EnvironmentShell;
use super::{structured_mut, Batch};

#[derive(Debug, Default)]
pub(super) struct CheckpointQueue(VecDeque<Option<GitObjectId>>);

impl CheckpointQueue {
    fn push(&mut self, checkpoint: Option<GitObjectId>) {
        self.0.push_back(checkpoint);
    }

    fn next_turn(&mut self) -> Option<GitObjectId> {
        self.0.pop_front().flatten()
    }

    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell::surface) fn group_folder(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
    ) -> Option<(HostLink, PathBuf)> {
        let root = self.envs.get(env)?.data.group_of(id)?.root.clone();
        Some((self.env_link(env), root))
    }

    pub(super) fn hand_over_after_checkpoint(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        batch: Batch,
        cx: &mut Context<Self>,
    ) {
        let Some((link, root)) = self.group_folder(env, id) else {
            return self.hand_over(env, id, batch);
        };
        if let Some(structured) = structured_mut(self, env, id) {
            structured.working = true;
        }
        let snapshot = cx.background_spawn(async move { snapshot_checkpoint(&link, &root) });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let taken = snapshot.await;
            this.update(cx, |shell, cx| {
                shell.checkpoint_taken(&env, id, taken);
                shell.hand_over(&env, id, batch);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn checkpoint_taken(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        taken: Result<GitObjectId, RepoError>,
    ) {
        let checkpoint = match taken {
            Ok(checkpoint) => Some(checkpoint),
            Err(error) => {
                tracing::debug!(terminal = id.as_u64(), %error, "no checkpoint before the turn");
                None
            }
        };
        if let Some(structured) = structured_mut(self, env, id) {
            structured.pending_checkpoints.push(checkpoint);
            structured.working = false;
        }
    }

    pub(super) fn note_prompted(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        event: &LiveEvent,
    ) {
        if let LiveEvent::Chat(ChatEvent::Prompted(point)) = event {
            self.keep_turn_checkpoint(env, id, point.clone());
        }
    }

    fn keep_turn_checkpoint(&mut self, env: &EnvironmentName, id: TerminalId, point: RewindPoint) {
        let Some(checkpoint) = structured_mut(self, env, id)
            .and_then(|structured| structured.pending_checkpoints.next_turn())
        else {
            return;
        };
        let Some(node) = self.chat_node(env, id) else {
            return;
        };
        node.checkpoints.record(point, checkpoint);
        self.store();
    }

    pub(super) fn forget_checkpoints_from(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        point: &RewindPoint,
    ) {
        let Some(node) = self.chat_node(env, id) else {
            return;
        };
        if node.checkpoints.before(point).is_none() {
            return;
        }
        node.checkpoints.cut_from(point);
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

    fn chat_node(&mut self, env: &EnvironmentName, id: TerminalId) -> Option<&mut TerminalNode> {
        self.envs
            .get_mut(env)?
            .data
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(raw: &str) -> Result<GitObjectId, krad_domain::ObjectIdError> {
        raw.parse()
    }

    #[test]
    fn two_quick_prompts_each_keep_their_own_checkpoint_in_order(
    ) -> Result<(), krad_domain::ObjectIdError> {
        let first = tree("4b825dc642cb6eb9a060e54bf8d69288fbee4904")?;
        let second = tree("0123456789abcdef0123456789abcdef01234567")?;
        let mut queue = CheckpointQueue::default();
        queue.push(Some(first.clone()));
        queue.push(None);
        queue.push(Some(second.clone()));
        assert_eq!(queue.next_turn(), Some(first));
        assert_eq!(queue.next_turn(), None);
        assert_eq!(queue.next_turn(), Some(second));
        assert_eq!(queue.next_turn(), None);
        queue.push(Some(tree("4b825dc642cb6eb9a060e54bf8d69288fbee4904")?));
        queue.clear();
        assert_eq!(queue.next_turn(), None);
        Ok(())
    }
}
