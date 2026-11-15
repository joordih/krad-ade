use futures::channel::{mpsc, oneshot};
use futures::{Stream, StreamExt as _};
use krad_agents::{Caller, Request, Response};
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::reply_with;
use crate::handle::RuntimeStopped;
use crate::input::Input;

pub(crate) const DELEGATION_BUFFER: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct DelegationId(u64);

#[derive(Debug)]
pub struct Delegated {
    pub from: Caller,
    pub request: Request,
    pub reply: BusReply,
}

type Respond = Box<dyn FnOnce(Response) -> bool + Send>;

enum Back {
    Runtime {
        delegation: DelegationId,
        inbox: Option<SyncSender<Input>>,
    },
    Remote(Option<Respond>),
}

pub struct BusReply {
    back: Back,
    withdrawn: Arc<AtomicBool>,
}

impl std::fmt::Debug for BusReply {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("BusReply").finish_non_exhaustive()
    }
}

impl BusReply {
    #[must_use]
    pub fn remote(respond: impl FnOnce(Response) -> bool + Send + 'static) -> Self {
        Self {
            back: Back::Remote(Some(Box::new(respond))),
            withdrawn: Arc::new(AtomicBool::new(false)),
        }
    }

    /// # Errors
    ///
    /// Fails when the runtime has stopped.
    pub fn send(mut self, response: Response) -> Result<(), RuntimeStopped> {
        match &mut self.back {
            Back::Runtime { delegation, inbox } => {
                let delegation = *delegation;
                let inbox = inbox.take().ok_or(RuntimeStopped)?;
                inbox
                    .send(Input::BusRespond {
                        delegation,
                        response,
                    })
                    .map_err(|_| RuntimeStopped)
            }
            Back::Remote(respond) => {
                let respond = respond.take().ok_or(RuntimeStopped)?;
                respond(response).then_some(()).ok_or(RuntimeStopped)
            }
        }
    }

    #[must_use]
    pub fn is_canceled(&self) -> bool {
        self.withdrawn.load(Ordering::Acquire)
    }
}

impl Drop for BusReply {
    fn drop(&mut self) {
        let Back::Runtime { delegation, inbox } = &mut self.back else {
            return;
        };
        let Some(inbox) = inbox.take() else {
            return;
        };
        if inbox.try_send(Input::BusDropped(*delegation)).is_err() {
            tracing::trace!("a dropped bus reply did not reach the runtime");
        }
    }
}

#[must_use]
#[derive(Debug)]
pub struct DelegationReceiver(pub(crate) mpsc::Receiver<Delegated>);

impl Stream for DelegationReceiver {
    type Item = Delegated;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Delegated>> {
        self.0.poll_next_unpin(cx)
    }
}

#[derive(Debug)]
struct Waiting {
    reply: oneshot::Sender<Response>,
    withdrawn: Arc<AtomicBool>,
}

#[derive(Debug)]
pub(crate) struct Delegations {
    inbox: SyncSender<Input>,
    last: DelegationId,
    waiting: HashMap<DelegationId, Waiting>,
    delegate: Option<mpsc::Sender<Delegated>>,
    backlog: VecDeque<Delegated>,
}

impl Delegations {
    pub(crate) fn new(inbox: SyncSender<Input>) -> Self {
        Self {
            inbox,
            last: DelegationId::default(),
            waiting: HashMap::new(),
            delegate: None,
            backlog: VecDeque::new(),
        }
    }

    pub(crate) fn delegate_to(&mut self, delegate: mpsc::Sender<Delegated>) {
        self.delegate = Some(delegate);
        self.hand_over();
    }

    pub(crate) fn delegate(
        &mut self,
        from: Caller,
        request: Request,
        reply: oneshot::Sender<Response>,
    ) {
        self.last = DelegationId(self.last.0 + 1);
        let withdrawn = Arc::new(AtomicBool::new(false));
        self.waiting.insert(
            self.last,
            Waiting {
                reply,
                withdrawn: Arc::clone(&withdrawn),
            },
        );
        self.backlog.push_back(Delegated {
            from,
            request,
            reply: BusReply {
                back: Back::Runtime {
                    delegation: self.last,
                    inbox: Some(self.inbox.clone()),
                },
                withdrawn,
            },
        });
        self.hand_over();
    }

    pub(crate) fn respond(&mut self, delegation: DelegationId, response: Response) {
        let Some(waiting) = self.waiting.remove(&delegation) else {
            tracing::trace!("a delegated request was answered after its caller left");
            return;
        };
        reply_with(waiting.reply, response);
    }

    pub(crate) fn forget(&mut self, delegation: DelegationId) {
        self.waiting.remove(&delegation);
    }

    pub(crate) fn tick(&mut self) {
        self.waiting.retain(|_, waiting| {
            let left = waiting.reply.is_canceled();
            if left {
                waiting.withdrawn.store(true, Ordering::Release);
            }
            !left
        });
        self.backlog
            .retain(|delegated| !delegated.reply.is_canceled());
        self.hand_over();
    }

    fn hand_over(&mut self) {
        let Some(delegate) = &mut self.delegate else {
            return;
        };
        while let Some(delegated) = self.backlog.pop_front() {
            match delegate.try_send(delegated) {
                Ok(()) => {}
                Err(error) if error.is_full() => {
                    self.backlog.push_front(error.into_inner());
                    return;
                }
                Err(error) => {
                    tracing::debug!("the bus delegate left; requests wait for the next one");
                    self.backlog.push_front(error.into_inner());
                    self.delegate = None;
                    return;
                }
            }
        }
    }
}
