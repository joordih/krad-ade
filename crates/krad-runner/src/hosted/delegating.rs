use futures::StreamExt as _;
use krad_domain::agents_api::Response;
use krad_protocol::{DelegatedRequest, DelegationId, ErrorCode, Message, Reply};
use krad_runtime::{BusReply, Delegated, RuntimeHandle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use super::connection::refused;
use super::outbox::Outbox;

const DELEGATION_THREAD: &str = "krad-runner-delegations";
const NO_DELEGATION: &str = "this connection holds no delegated request with that id";
const STOPPED: &str = "the runtime has stopped";
const NO_THREAD: &str = "delegated requests could not be forwarded";

#[derive(Default)]
struct Taken {
    last: u64,
    replies: HashMap<DelegationId, BusReply>,
}

#[derive(Clone, Default)]
pub(crate) struct Delegations(Arc<Mutex<Taken>>);

impl Delegations {
    fn taken(&self) -> MutexGuard<'_, Taken> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn keep(&self, reply: BusReply) -> DelegationId {
        let mut taken = self.taken();
        taken.last += 1;
        let delegation = DelegationId(taken.last);
        taken.replies.insert(delegation, reply);
        delegation
    }

    pub(crate) fn respond(&self, delegation: DelegationId, response: Response) -> Reply {
        let Some(reply) = self.taken().replies.remove(&delegation) else {
            return refused(ErrorCode::NotFound, NO_DELEGATION);
        };
        match reply.send(response) {
            Ok(()) => Reply::Accepted,
            Err(_) => refused(ErrorCode::Unavailable, STOPPED),
        }
    }
}

pub(crate) fn take_delegations(
    handle: &RuntimeHandle,
    outbox: &Outbox,
    delegations: &Delegations,
) -> Option<Message> {
    let mut incoming = handle.delegate_bus();
    let (outbox, delegations) = (outbox.clone(), delegations.clone());
    let spawned = thread::Builder::new()
        .name(DELEGATION_THREAD.to_owned())
        .spawn(move || {
            while let Some(delegated) = futures::executor::block_on(incoming.next()) {
                let Delegated {
                    from,
                    request,
                    reply,
                } = delegated;
                let delegation = delegations.keep(reply);
                let message = Message::Delegated(Box::new(DelegatedRequest {
                    delegation,
                    from,
                    request,
                }));
                if !outbox.stream(message) {
                    return;
                }
            }
        });
    spawned.err().map(|error| {
        tracing::warn!(%error, "{NO_THREAD}");
        Message::Refused {
            code: ErrorCode::Unavailable,
            message: NO_THREAD.to_owned(),
        }
    })
}
