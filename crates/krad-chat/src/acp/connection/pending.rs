use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::AcpError;
use crate::acp::rpc::RpcError;

pub(super) type Answer = Result<Value, RpcError>;

#[derive(Debug, Default)]
pub(super) struct Waiters {
    closed: bool,
    waiting: HashMap<u64, SyncSender<Answer>>,
}

impl Waiters {
    pub(super) fn wait_for(&mut self, id: u64, answered: SyncSender<Answer>) -> bool {
        if self.closed {
            return false;
        }
        self.waiting.insert(id, answered);
        true
    }

    pub(super) fn forget(&mut self, id: u64) {
        self.waiting.remove(&id);
    }

    pub(super) fn settle(&mut self, id: u64) -> Option<SyncSender<Answer>> {
        self.waiting.remove(&id)
    }

    pub(super) fn close(&mut self) {
        self.closed = true;
        self.waiting.clear();
    }
}

#[must_use]
#[derive(Debug)]
pub struct Pending<R> {
    id: u64,
    answer: Receiver<Answer>,
    waiters: Arc<Mutex<Waiters>>,
    answer_type: PhantomData<fn() -> R>,
}

impl<R> Pending<R> {
    pub(super) const fn new(
        id: u64,
        answer: Receiver<Answer>,
        waiters: Arc<Mutex<Waiters>>,
    ) -> Self {
        Self {
            id,
            answer,
            waiters,
            answer_type: PhantomData,
        }
    }
}

impl<R: DeserializeOwned> Pending<R> {
    /// # Errors
    ///
    /// Returns [`AcpError::Refused`] when the agent answers with an error, [`AcpError::Gone`] when
    /// its output ends first and [`AcpError::Shape`] when the answer does not read as `R`.
    pub fn wait(self) -> Result<R, AcpError> {
        let answer = self.answer.recv().map_err(|_| AcpError::Gone)?;
        read_answer(answer)
    }

    /// # Errors
    ///
    /// As [`Pending::wait`], and [`AcpError::Timeout`] when no answer comes within `timeout`.
    pub fn wait_timeout(self, timeout: Duration) -> Result<R, AcpError> {
        let answer = self
            .answer
            .recv_timeout(timeout)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => AcpError::Timeout,
                RecvTimeoutError::Disconnected => AcpError::Gone,
            })?;
        read_answer(answer)
    }
}

impl<R> Drop for Pending<R> {
    fn drop(&mut self) {
        self.waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .forget(self.id);
    }
}

fn read_answer<R: DeserializeOwned>(answer: Answer) -> Result<R, AcpError> {
    let value = answer.map_err(AcpError::Refused)?;
    Ok(serde_json::from_value(value)?)
}
