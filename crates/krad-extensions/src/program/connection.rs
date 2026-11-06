mod router;

use futures::channel::{mpsc, oneshot};
use krad_extension_api::protocol::{Notification, Request};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::program::calls::{AppCall, AppNotice};
use crate::program::error::ProgramError;
use crate::program::rpc::{read_message, Outgoing, ResponseError, REQUEST_FAILED};
use router::{Routed, Router};

const ROUTER_QUEUE: usize = 256;
const EVENT_QUEUE: usize = 64;
const READER_THREAD: &str = "krad-extension-reader";
const ROUTER_THREAD: &str = "krad-extension-router";
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub struct CallId(Value);

#[derive(Debug, Clone, PartialEq)]
pub enum ProgramEvent {
    Call { id: CallId, call: AppCall },
    Notice(AppNotice),
    Stopped,
}

#[derive(Clone)]
pub struct ProgramConnection {
    routed: SyncSender<Routed>,
    next_id: Arc<AtomicI64>,
}

impl ProgramConnection {
    /// # Errors
    ///
    /// Returns [`ProgramError::Spawn`] when the reader or router thread cannot start.
    pub fn over<R, W>(
        reader: R,
        writer: W,
        timeout: Duration,
    ) -> Result<(Self, mpsc::Receiver<ProgramEvent>), ProgramError>
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let (outbox, inbox) = sync_channel(ROUTER_QUEUE);
        let (events, listener) = mpsc::channel(EVENT_QUEUE);
        let router = Router::new(writer, events, timeout);
        thread::Builder::new()
            .name(ROUTER_THREAD.to_owned())
            .spawn(move || router.run(&inbox))
            .map_err(ProgramError::Spawn)?;
        let incoming = outbox.clone();
        thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || read_into(reader, &incoming))
            .map_err(ProgramError::Spawn)?;
        let connection = Self {
            routed: outbox,
            next_id: Arc::new(AtomicI64::new(1)),
        };
        Ok((connection, listener))
    }

    fn send(&self, routed: Routed) -> Result<(), ProgramError> {
        self.routed.send(routed).map_err(|_| ProgramError::Stopped)
    }

    /// # Errors
    ///
    /// Returns [`ProgramError`] when the program stops, refuses, answers late or answers with
    /// data of another shape.
    pub async fn request<R: Request>(&self, params: &R::Params) -> Result<R::Result, ProgramError> {
        let (reply, answer) = oneshot::channel();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.send(Routed::Out {
            message: Outgoing::Request {
                id,
                method: R::METHOD,
                params: serde_json::to_value(params)?,
            },
            reply: Some((id, reply)),
        })?;
        let value = answer.await.map_err(|_| ProgramError::Stopped)??;
        Ok(serde_json::from_value(value)?)
    }

    /// # Errors
    ///
    /// Returns [`ProgramError`] when the program has stopped.
    pub fn notify<N: Notification>(&self, params: &N::Params) -> Result<(), ProgramError> {
        self.send(Routed::Out {
            message: Outgoing::Notification {
                method: N::METHOD,
                params: serde_json::to_value(params)?,
            },
            reply: None,
        })
    }

    /// # Errors
    ///
    /// Returns [`ProgramError`] when the program has stopped.
    pub fn answer<R: Request>(&self, id: CallId, result: &R::Result) -> Result<(), ProgramError> {
        self.reply(id, Ok(serde_json::to_value(result)?))
    }

    /// # Errors
    ///
    /// Returns [`ProgramError`] when the program has stopped.
    pub fn refuse(&self, id: CallId, message: &str) -> Result<(), ProgramError> {
        self.reply(
            id,
            Err(ResponseError {
                code: REQUEST_FAILED,
                message: message.to_owned(),
            }),
        )
    }

    fn reply(&self, id: CallId, outcome: Result<Value, ResponseError>) -> Result<(), ProgramError> {
        self.send(Routed::Out {
            message: Outgoing::Reply { id: id.0, outcome },
            reply: None,
        })
    }
}

fn read_into(mut reader: impl BufRead, routed: &SyncSender<Routed>) {
    loop {
        match read_message(&mut reader) {
            Ok(Some(message)) => {
                if routed.send(Routed::Incoming(message)).is_err() {
                    return;
                }
            }
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "extension program sent an unreadable frame");
                break;
            }
        }
    }
    routed.send(Routed::Closed).unwrap_or_default();
}

#[cfg(test)]
mod tests;
