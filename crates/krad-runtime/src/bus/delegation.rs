use futures::channel::{mpsc, oneshot};
use futures::{Stream, StreamExt as _};
use krad_agents::{Caller, Request, Response, TerminalHandle};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mail {
    pub from: TerminalHandle,
    pub from_title: String,
    pub to: u64,
    pub body: String,
}

#[derive(Debug)]
pub struct BusReply {
    delegation: DelegationId,
    inbox: Option<SyncSender<Input>>,
    withdrawn: Arc<AtomicBool>,
}

impl BusReply {
    /// # Errors
    ///
    /// Fails when the runtime has stopped.
    pub fn send(mut self, response: Response) -> Result<(), RuntimeStopped> {
        let delegation = self.delegation;
        self.hand_back(Input::BusRespond {
            delegation,
            response,
        })
    }

    /// # Errors
    ///
    /// Fails when the runtime has stopped.
    pub fn post_mail(mut self, mail: Mail) -> Result<(), RuntimeStopped> {
        let delegation = self.delegation;
        self.hand_back(Input::BusMail { delegation, mail })
    }

    #[must_use]
    pub fn is_canceled(&self) -> bool {
        self.withdrawn.load(Ordering::Acquire)
    }

    fn hand_back(&mut self, input: Input) -> Result<(), RuntimeStopped> {
        let inbox = self.inbox.take().ok_or(RuntimeStopped)?;
        inbox.send(input).map_err(|_| RuntimeStopped)
    }
}

impl Drop for BusReply {
    fn drop(&mut self) {
        let Some(inbox) = self.inbox.take() else {
            return;
        };
        if inbox.try_send(Input::BusDropped(self.delegation)).is_err() {
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
                delegation: self.last,
                inbox: Some(self.inbox.clone()),
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
