use futures::channel::oneshot;
use krad_protocol::Reply;
use std::sync::Arc;

use super::{answer_to, Runtime};
use crate::terminals::Settling;

#[derive(Debug)]
struct Settled(Option<oneshot::Sender<Reply>>);

impl Drop for Settled {
    fn drop(&mut self) {
        if let Some(reply) = self.0.take() {
            answer_to(reply, Reply::Accepted);
        }
    }
}

impl Runtime {
    pub(super) fn settle(&mut self, reply: oneshot::Sender<Reply>) {
        self.save_terminal_metas(Settling::Now);
        let settled = Arc::new(Settled(Some(reply)));
        let waiting = Arc::clone(&settled);
        self.settings.settle(move || drop(waiting));
        let shelved = Arc::clone(&settled);
        self.kraddies.flush(&mut self.outbox);
        self.kraddies.settle(move || drop(shelved));
        self.environments.settle(move || drop(settled));
    }
}
