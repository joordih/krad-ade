use krad_domain::{GitObjectId, RewindPoint};
use krad_protocol::{AgentRef, EventKind};

use super::chat::{Batch, Chat};
use super::{hand_over, ChatWork, Chats};
use crate::jobs::{JobDone, Jobs};

const STOPPED_TEXT: &str = "The chat stopped before the message reached it";

#[derive(Debug)]
pub(crate) struct ChatCheckpoint {
    pub(crate) chat: AgentRef,
    pub(crate) opened: u64,
    pub(crate) taken: Option<GitObjectId>,
}

pub(super) fn take_first(chat: &mut Chat, batch: Batch, jobs: &Jobs) -> bool {
    let Some(root) = chat.place.root.clone() else {
        return false;
    };
    if batch.message.is_none() || !chat.harness.checkpoints_files() {
        return false;
    }
    let link = chat.place.link.clone();
    let (id, opened) = (chat.id.clone(), chat.opened);
    let started = jobs.run(move || {
        let taken = krad_repo::snapshot_checkpoint(&link, &root)
            .inspect_err(|error| tracing::debug!(%error, "no checkpoint before the turn"))
            .ok();
        JobDone::ChatCheckpoint(ChatCheckpoint {
            chat: id,
            opened,
            taken,
        })
    });
    if let Err(error) = started {
        tracing::warn!(%error, "the checkpoint job could not start");
        chat.checkpoints.push_back(None);
        return false;
    }
    chat.begin_work();
    chat.checkpointing = Some(batch);
    true
}

pub(super) fn prompted(chat: &mut Chat, point: &RewindPoint, outbox: &mut Vec<EventKind>) {
    let Some(Some(checkpoint)) = chat.checkpoints.pop_front() else {
        return;
    };
    outbox.push(EventKind::ChatCheckpointed {
        chat: chat.id.clone(),
        point: point.clone(),
        checkpoint,
    });
}

impl Chats {
    pub(crate) fn checkpoint_taken(&mut self, taken: ChatCheckpoint, work: &mut ChatWork<'_>) {
        let Some(chat) = self.open.get_mut(&taken.chat) else {
            return;
        };
        if chat.opened != taken.opened {
            return;
        }
        let Some(batch) = chat.checkpointing.take() else {
            return;
        };
        if chat.live().is_some() {
            chat.checkpoints.push_back(taken.taken);
            hand_over(chat, batch);
        } else {
            chat.end_work();
            chat.problem = Some(STOPPED_TEXT.to_owned());
        }
        self.flush(&taken.chat, work);
    }
}
