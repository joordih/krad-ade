use serde::de::DeserializeOwned;
use serde_json::Value;
use std::io::Read;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, PoisonError};
use tracing::{debug, trace};

use super::pending::{Answer, Waiters};
use super::responder::{AgentRequest, ReplyTo};
use super::AcpClient;
use crate::acp::methods;
use crate::acp::rpc::{ErrorCode, Incoming, RequestId, RpcError};
use crate::acp::update::{SessionNotification, SessionUpdate};
use crate::cloud::{HarnessLines, LineOrigin};

const UPDATE_KIND_POINTER: &str = "/update/sessionUpdate";
const UNKNOWN_METHOD_TEXT: &str = "Krad ADE does not handle this method";

pub(super) struct Router<C> {
    waiters: Arc<Mutex<Waiters>>,
    outbox: SyncSender<String>,
    client: C,
}

impl<C: AcpClient> Router<C> {
    pub(super) const fn new(
        waiters: Arc<Mutex<Waiters>>,
        outbox: SyncSender<String>,
        client: C,
    ) -> Self {
        Self {
            waiters,
            outbox,
            client,
        }
    }

    pub(super) fn read(mut self, input: HarnessLines<impl Read>) {
        for line in input {
            match line {
                Ok(line) => self.route(&line.text, line.origin),
                Err(error) => {
                    debug!(%error, "the ACP agent output ended with an error");
                    break;
                }
            }
        }
        self.waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .close();
        self.client.closed();
    }

    fn route(&mut self, line: &str, origin: LineOrigin) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let incoming = match serde_json::from_str::<Incoming>(line) {
            Ok(incoming) => incoming,
            Err(error) => {
                debug!(category = ?error.classify(), "skipped a line from the ACP agent that is not JSON-RPC");
                return;
            }
        };
        if origin.is_replayed() && !matches!(incoming, Incoming::Request { .. }) {
            trace!("skipped a replayed ACP message that is not a request");
            return;
        }
        match incoming {
            Incoming::Success { id, result } => self.settle(&id, Ok(result)),
            Incoming::Failure { id, error } => self.settle(&id, Err(error)),
            Incoming::Notification { method, params } => self.notified(&method, params),
            Incoming::Request { id, method, params } => self.requested(id, &method, params),
        }
    }

    fn settle(&self, id: &RequestId, answer: Answer) {
        let RequestId::Number(number) = id else {
            debug!(
                ?id,
                "skipped an ACP answer to a request the app did not send"
            );
            return;
        };
        let waiter = u64::try_from(*number).ok().and_then(|number| {
            self.waiters
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .settle(number)
        });
        let Some(waiter) = waiter else {
            debug!(?id, "skipped an ACP answer nobody waits for");
            return;
        };
        if waiter.send(answer).is_err() {
            debug!(?id, "an ACP answer came after its caller left");
        }
    }

    fn notified(&mut self, method: &str, params: Value) {
        if method != methods::SESSION_UPDATE {
            debug!(
                method,
                "skipped an ACP notification the app does not handle"
            );
            return;
        }
        let kind = params
            .pointer(UPDATE_KIND_POINTER)
            .and_then(Value::as_str)
            .map(str::to_owned);
        match serde_json::from_value::<SessionNotification>(params) {
            Ok(SessionNotification {
                update: SessionUpdate::Unknown,
                ..
            }) => debug!(?kind, "skipped an ACP session update the app does not know"),
            Ok(notification) => {
                trace!(?kind, "ACP session update");
                self.client.session_update(notification);
            }
            Err(error) => debug!(
                ?kind,
                category = ?error.classify(),
                "skipped an ACP session update the app cannot read"
            ),
        }
    }

    fn requested(&mut self, id: RequestId, method: &str, params: Value) {
        let reply_to = ReplyTo::new(id, self.outbox.clone());
        match read_request(method, params, &reply_to) {
            Ok(request) => self.client.request(request),
            Err(error) => {
                debug!(method, code = error.code.0, "refused an ACP request");
                reply_to.send_error(&error);
            }
        }
    }
}

fn read_request(method: &str, params: Value, reply_to: &ReplyTo) -> Result<AgentRequest, RpcError> {
    Ok(match method {
        methods::SESSION_REQUEST_PERMISSION => {
            AgentRequest::RequestPermission(read_params(params)?, reply_to.responder())
        }
        methods::FS_READ_TEXT_FILE => {
            AgentRequest::ReadTextFile(read_params(params)?, reply_to.responder())
        }
        methods::FS_WRITE_TEXT_FILE => {
            AgentRequest::WriteTextFile(read_params(params)?, reply_to.responder())
        }
        methods::TERMINAL_CREATE => {
            AgentRequest::CreateTerminal(read_params(params)?, reply_to.responder())
        }
        methods::TERMINAL_OUTPUT => {
            AgentRequest::TerminalOutput(read_params(params)?, reply_to.responder())
        }
        methods::TERMINAL_WAIT_FOR_EXIT => {
            AgentRequest::WaitForTerminalExit(read_params(params)?, reply_to.responder())
        }
        methods::TERMINAL_KILL => {
            AgentRequest::KillTerminal(read_params(params)?, reply_to.responder())
        }
        methods::TERMINAL_RELEASE => {
            AgentRequest::ReleaseTerminal(read_params(params)?, reply_to.responder())
        }
        _ => {
            return Err(RpcError::new(
                ErrorCode::METHOD_NOT_FOUND,
                UNKNOWN_METHOD_TEXT,
            ))
        }
    })
}

fn read_params<P: DeserializeOwned>(params: Value) -> Result<P, RpcError> {
    serde_json::from_value(params)
        .map_err(|error| RpcError::new(ErrorCode::INVALID_PARAMS, error.to_string()))
}
