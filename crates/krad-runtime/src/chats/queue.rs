use krad_chat::LiveEvent;
use krad_protocol::AgentRef;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};

use crate::input::Input;

const CHAT_QUEUE_CAPACITY: usize = 256;

#[derive(Debug)]
pub(crate) struct Heard {
    pub(crate) chat: AgentRef,
    pub(crate) generation: u64,
    pub(crate) event: LiveEvent,
}

#[derive(Clone, Debug)]
pub(crate) struct ChatQueue {
    heard: SyncSender<Heard>,
    wake: SyncSender<Input>,
}

impl ChatQueue {
    pub(crate) fn open(wake: SyncSender<Input>) -> (Self, Receiver<Heard>) {
        let (heard, received) = sync_channel(CHAT_QUEUE_CAPACITY);
        (Self { heard, wake }, received)
    }

    pub(crate) fn sink(&self, chat: AgentRef, generation: u64) -> impl FnMut(LiveEvent) + Send {
        let queue = self.clone();
        move |event| {
            let heard = Heard {
                chat: chat.clone(),
                generation,
                event,
            };
            if queue.heard.send(heard).is_err() {
                tracing::trace!("a chat event arrived after the runtime stopped");
                return;
            }
            if let Err(TrySendError::Disconnected(_)) = queue.wake.try_send(Input::Wake) {
                tracing::trace!("the runtime stopped before a chat event woke it");
            }
        }
    }
}
