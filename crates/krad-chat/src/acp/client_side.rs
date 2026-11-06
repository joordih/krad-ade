mod files;
mod terminals;

use krad_host::HostLink;
use serde::Serialize;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;

pub use files::{FileWritten, OpenBuffers};
pub use terminals::HostTerminals;

use super::connection::{AgentRequest, Responder};
use super::paths::AgentPaths;
use super::permission::{RequestPermissionRequest, RequestPermissionResponse};
use super::rpc::{ErrorCode, RpcError};
use files::HostFiles;

const RELATIVE_PATH_TEXT: &str = "Paths must be absolute";

pub type PermissionAsked = (
    RequestPermissionRequest,
    Responder<RequestPermissionResponse>,
);

pub struct ClientSide {
    files: Arc<HostFiles>,
    pub terminals: HostTerminals,
}

impl ClientSide {
    #[must_use]
    pub fn new(
        host: HostLink,
        paths: AgentPaths,
        buffers: Arc<dyn OpenBuffers>,
        written: Sender<FileWritten>,
    ) -> Self {
        Self {
            files: Arc::new(HostFiles::new(
                host.clone(),
                paths.clone(),
                buffers,
                written,
            )),
            terminals: HostTerminals::new(host, paths),
        }
    }

    #[must_use]
    pub fn serve(&self, request: AgentRequest) -> Option<PermissionAsked> {
        match request {
            AgentRequest::ReadTextFile(read, responder) => {
                let files = Arc::clone(&self.files);
                thread::spawn(move || answer(responder, files.read(&read)));
            }
            AgentRequest::WriteTextFile(write, responder) => {
                let files = Arc::clone(&self.files);
                thread::spawn(move || answer(responder, files.write(write)));
            }
            AgentRequest::CreateTerminal(create, responder) => {
                let terminals = self.terminals.clone();
                thread::spawn(move || answer(responder, terminals.create(&create)));
            }
            AgentRequest::TerminalOutput(asked, responder) => {
                answer(responder, self.terminals.output(&asked.terminal_id));
            }
            AgentRequest::WaitForTerminalExit(asked, responder) => {
                let terminals = self.terminals.clone();
                thread::spawn(move || {
                    answer(responder, terminals.wait_for_exit(&asked.terminal_id));
                });
            }
            AgentRequest::KillTerminal(asked, responder) => {
                let terminals = self.terminals.clone();
                thread::spawn(move || answer(responder, terminals.kill(&asked.terminal_id)));
            }
            AgentRequest::ReleaseTerminal(asked, responder) => {
                let terminals = self.terminals.clone();
                thread::spawn(move || answer(responder, terminals.release(&asked.terminal_id)));
            }
            AgentRequest::RequestPermission(asked, responder) => return Some((asked, responder)),
        }
        None
    }
}

fn answer<R: Serialize>(responder: Responder<R>, result: Result<R, RpcError>) {
    match result {
        Ok(result) => responder.reply(&result),
        Err(error) => responder.fail(&error),
    }
}

fn absolute(paths: &AgentPaths, host: &HostLink, path: &Path) -> Result<(), RpcError> {
    if paths.is_absolute(host, path) {
        return Ok(());
    }
    Err(RpcError::new(ErrorCode::INVALID_PARAMS, RELATIVE_PATH_TEXT))
}

#[cfg(test)]
mod tests;
