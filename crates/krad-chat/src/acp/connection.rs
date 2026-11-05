mod pending;
mod responder;
mod router;

use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use thiserror::Error;
use tracing::debug;

pub use pending::Pending;
pub use responder::{AgentRequest, Responder};

use super::methods;
use super::prompt::{CancelNotification, PromptRequest, PromptResponse};
use super::rpc::{EmptyResponse, OutgoingNotification, OutgoingRequest, RpcError};
use super::setup::{
    AuthenticateRequest, InitializeRequest, InitializeResponse, LoadSessionRequest,
    LoadSessionResponse, NewSessionRequest, NewSessionResponse, SetSessionModeRequest,
    SetSessionModelRequest,
};
use super::update::SessionNotification;
use crate::control::json_line;
use pending::Waiters;
use router::Router;

const OUTBOX_CAPACITY: usize = 64;

#[derive(Debug, Error)]
pub enum AcpError {
    #[error("the agent refused the request: {0}")]
    Refused(RpcError),
    #[error("the agent stopped answering")]
    Gone,
    #[error("the agent did not answer in time")]
    Timeout,
    #[error("the agent sent an answer the app cannot read: {0}")]
    Shape(#[from] serde_json::Error),
}

pub trait AcpClient: Send + 'static {
    fn session_update(&mut self, notification: SessionNotification);
    fn request(&mut self, request: AgentRequest);
    fn closed(&mut self);
}

pub struct AcpConnection {
    outbox: SyncSender<String>,
    waiters: Arc<Mutex<Waiters>>,
    next_id: AtomicU64,
}

impl AcpConnection {
    #[tracing::instrument(skip_all)]
    pub fn start<R, W, C>(input: R, output: W, client: C) -> Self
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
        C: AcpClient,
    {
        let (outbox, lines) = sync_channel::<String>(OUTBOX_CAPACITY);
        let waiters = Arc::new(Mutex::new(Waiters::default()));
        thread::spawn(move || write_lines(output, &lines));
        let router = Router::new(Arc::clone(&waiters), outbox.clone(), client);
        thread::spawn(move || router.read(input));
        Self {
            outbox,
            waiters,
            next_id: AtomicU64::new(0),
        }
    }

    pub fn initialize(&self, request: &InitializeRequest) -> Pending<InitializeResponse> {
        self.call(methods::INITIALIZE, request)
    }

    pub fn authenticate(&self, request: &AuthenticateRequest) -> Pending<EmptyResponse> {
        self.call(methods::AUTHENTICATE, request)
    }

    pub fn new_session(&self, request: &NewSessionRequest) -> Pending<NewSessionResponse> {
        self.call(methods::SESSION_NEW, request)
    }

    pub fn load_session(&self, request: &LoadSessionRequest) -> Pending<LoadSessionResponse> {
        self.call(methods::SESSION_LOAD, request)
    }

    pub fn prompt(&self, request: &PromptRequest) -> Pending<PromptResponse> {
        self.call(methods::SESSION_PROMPT, request)
    }

    pub fn set_mode(&self, request: &SetSessionModeRequest) -> Pending<EmptyResponse> {
        self.call(methods::SESSION_SET_MODE, request)
    }

    pub fn set_model(&self, request: &SetSessionModelRequest) -> Pending<EmptyResponse> {
        self.call(methods::SESSION_SET_MODEL, request)
    }

    pub fn cancel(&self, notification: &CancelNotification) {
        let line = json_line(&OutgoingNotification::new(
            methods::SESSION_CANCEL,
            notification,
        ));
        if self.outbox.send(line).is_err() {
            debug!("the ACP agent is gone before a cancel");
        }
    }

    fn call<P: Serialize, R>(&self, method: &'static str, params: &P) -> Pending<R> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (answered, answer) = sync_channel(1);
        let open = self
            .waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .wait_for(id, answered);
        let pending = Pending::new(id, answer, Arc::clone(&self.waiters));
        if !open {
            debug!(method, "the ACP agent is gone before a request");
            return pending;
        }
        let line = json_line(&OutgoingRequest::new(id, method, params));
        if self.outbox.send(line).is_err() {
            debug!(method, "the ACP agent stopped reading before a request");
            self.waiters
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .forget(id);
        }
        pending
    }
}

fn write_lines(mut output: impl Write, lines: &Receiver<String>) {
    while let Ok(line) = lines.recv() {
        if let Err(error) = output
            .write_all(line.as_bytes())
            .and_then(|()| output.flush())
        {
            debug!(%error, "the ACP agent stopped reading");
            break;
        }
    }
}

#[cfg(test)]
mod tests;
