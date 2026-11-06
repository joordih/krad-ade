use serde::Serialize;
use std::marker::PhantomData;
use std::sync::mpsc::SyncSender;
use tracing::debug;

use crate::acp::files::{ReadTextFileRequest, ReadTextFileResponse, WriteTextFileRequest};
use crate::acp::permission::{RequestPermissionRequest, RequestPermissionResponse};
use crate::acp::rpc::{
    EmptyResponse, ErrorCode, OutgoingError, OutgoingResult, RequestId, RpcError,
};
use crate::acp::terminals::{
    CreateTerminalRequest, CreateTerminalResponse, TerminalExitStatus, TerminalOutputResponse,
    TerminalRequest,
};
use crate::control::json_line;

const UNANSWERED_TEXT: &str = "the client dropped the request without answering";

#[derive(Debug)]
pub enum AgentRequest {
    RequestPermission(
        RequestPermissionRequest,
        Responder<RequestPermissionResponse>,
    ),
    ReadTextFile(ReadTextFileRequest, Responder<ReadTextFileResponse>),
    WriteTextFile(WriteTextFileRequest, Responder<EmptyResponse>),
    CreateTerminal(CreateTerminalRequest, Responder<CreateTerminalResponse>),
    TerminalOutput(TerminalRequest, Responder<TerminalOutputResponse>),
    WaitForTerminalExit(TerminalRequest, Responder<TerminalExitStatus>),
    KillTerminal(TerminalRequest, Responder<EmptyResponse>),
    ReleaseTerminal(TerminalRequest, Responder<EmptyResponse>),
}

#[derive(Clone, Debug)]
pub(super) struct ReplyTo {
    id: RequestId,
    outbox: SyncSender<String>,
}

impl ReplyTo {
    pub(super) const fn new(id: RequestId, outbox: SyncSender<String>) -> Self {
        Self { id, outbox }
    }

    pub(super) fn responder<R>(&self) -> Responder<R> {
        Responder {
            reply_to: Some(self.clone()),
            answer_type: PhantomData,
        }
    }

    fn send_result(&self, result: &impl Serialize) {
        self.send(json_line(&OutgoingResult::new(&self.id, result)));
    }

    pub(super) fn send_error(&self, error: &RpcError) {
        self.send(json_line(&OutgoingError::new(&self.id, error)));
    }

    fn send(&self, line: String) {
        if self.outbox.send(line).is_err() {
            debug!(id = ?self.id, "the ACP agent is gone before an answer");
        }
    }
}

#[must_use]
#[derive(Debug)]
pub struct Responder<R> {
    reply_to: Option<ReplyTo>,
    answer_type: PhantomData<fn(R)>,
}

impl<R: Serialize> Responder<R> {
    pub fn reply(mut self, result: &R) {
        if let Some(reply_to) = self.reply_to.take() {
            reply_to.send_result(result);
        }
    }
}

impl<R> Responder<R> {
    pub fn fail(mut self, error: &RpcError) {
        if let Some(reply_to) = self.reply_to.take() {
            reply_to.send_error(error);
        }
    }
}

impl<R> Drop for Responder<R> {
    fn drop(&mut self) {
        if let Some(reply_to) = self.reply_to.take() {
            reply_to.send_error(&RpcError::new(ErrorCode::INTERNAL_ERROR, UNANSWERED_TEXT));
        }
    }
}
