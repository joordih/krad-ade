use futures::channel::{mpsc, oneshot};
use futures::executor::block_on;
use futures::SinkExt as _;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::{CallId, ProgramEvent};
use crate::program::calls::{AppCall, AppNotice};
use crate::program::error::ProgramError;
use crate::program::rpc::{
    write_message, Incoming, Outgoing, ResponseError, INVALID_PARAMS, METHOD_NOT_FOUND,
};

const SWEEP_INTERVAL: Duration = Duration::from_millis(250);

type Reply = oneshot::Sender<Result<Value, ProgramError>>;

pub(super) enum Routed {
    Out {
        message: Outgoing,
        reply: Option<(i64, Reply)>,
    },
    Incoming(Incoming),
    Closed,
}

struct Pending {
    reply: Reply,
    deadline: Instant,
}

pub(super) struct Router<W: Write> {
    output: W,
    events: mpsc::Sender<ProgramEvent>,
    pending: HashMap<i64, Pending>,
    timeout: Duration,
}

impl<W: Write> Router<W> {
    pub(super) fn new(output: W, events: mpsc::Sender<ProgramEvent>, timeout: Duration) -> Self {
        Self {
            output,
            events,
            pending: HashMap::new(),
            timeout,
        }
    }

    pub(super) fn run(mut self, routed: &Receiver<Routed>) {
        loop {
            match routed.recv_timeout(SWEEP_INTERVAL) {
                Ok(Routed::Out { message, reply }) => self.write(&message, reply),
                Ok(Routed::Incoming(message)) => self.incoming(message),
                Ok(Routed::Closed) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.sweep(Instant::now());
        }
        for (_, pending) in self.pending.drain() {
            pending
                .reply
                .send(Err(ProgramError::Stopped))
                .unwrap_or_default();
        }
        self.emit(ProgramEvent::Stopped);
    }

    fn write(&mut self, message: &Outgoing, reply: Option<(i64, Reply)>) {
        if let Err(error) = write_message(&mut self.output, message) {
            tracing::debug!(%error, "extension program pipe closed");
            if let Some((_, reply)) = reply {
                reply.send(Err(ProgramError::Stopped)).unwrap_or_default();
            }
            return;
        }
        if let Some((id, reply)) = reply {
            let deadline = Instant::now() + self.timeout;
            self.pending.insert(id, Pending { reply, deadline });
        }
    }

    fn sweep(&mut self, now: Instant) {
        let late: Vec<i64> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in late {
            if let Some(pending) = self.pending.remove(&id) {
                pending
                    .reply
                    .send(Err(ProgramError::Timeout))
                    .unwrap_or_default();
            }
        }
    }

    fn incoming(&mut self, message: Incoming) {
        match message {
            Incoming::Response { id, outcome } => {
                if let Some(pending) = self.pending.remove(&id) {
                    let outcome = outcome.map_err(ProgramError::Refused);
                    pending.reply.send(outcome).unwrap_or_default();
                }
            }
            Incoming::Request { id, method, params } => match AppCall::parse(&method, params) {
                Some(Ok(call)) => self.emit(ProgramEvent::Call {
                    id: CallId(id),
                    call,
                }),
                Some(Err(error)) => self.refuse(id, INVALID_PARAMS, error.to_string()),
                None => self.refuse(id, METHOD_NOT_FOUND, method),
            },
            Incoming::Notification { method, params } => match AppNotice::parse(&method, params) {
                Some(Ok(notice)) => self.emit(ProgramEvent::Notice(notice)),
                Some(Err(error)) => {
                    tracing::debug!(%method, %error, "extension notification with bad params");
                }
                None => tracing::trace!(%method, "unknown extension notification"),
            },
        }
    }

    fn refuse(&mut self, id: Value, code: i64, message: String) {
        let reply = Outgoing::Reply {
            id,
            outcome: Err(ResponseError { code, message }),
        };
        self.write(&reply, None);
    }

    fn emit(&mut self, event: ProgramEvent) {
        if block_on(self.events.send(event)).is_err() {
            tracing::trace!("extension events dropped after the app stopped listening");
        }
    }
}
