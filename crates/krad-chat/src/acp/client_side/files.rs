use krad_domain::{line_diff, FileEdit};
use krad_host::{HostError, HostLink};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tracing::debug;

use super::absolute;
use crate::acp::files::{ReadTextFileRequest, ReadTextFileResponse, WriteTextFileRequest};
use crate::acp::ids::SessionId;
use crate::acp::paths::AgentPaths;
use crate::acp::rpc::{EmptyResponse, ErrorCode, RpcError};

const FIRST_LINE: u32 = 1;
const LINE_END: char = '\n';
const READ_FAILED_TEXT: &str = "The file could not be read";
const WRITE_FAILED_TEXT: &str = "The file could not be written";

pub trait OpenBuffers: Send + Sync + 'static {
    fn unsaved_text(&self, path: &Path) -> Option<String>;
    fn written(&self, path: &Path, content: &str);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileWritten {
    pub session_id: SessionId,
    pub path: PathBuf,
    pub edit: FileEdit,
}

pub(super) struct HostFiles {
    host: HostLink,
    paths: AgentPaths,
    buffers: Arc<dyn OpenBuffers>,
    written: Sender<FileWritten>,
}

impl HostFiles {
    pub(super) fn new(
        host: HostLink,
        paths: AgentPaths,
        buffers: Arc<dyn OpenBuffers>,
        written: Sender<FileWritten>,
    ) -> Self {
        Self {
            host,
            paths,
            buffers,
            written,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn read(
        &self,
        request: &ReadTextFileRequest,
    ) -> Result<ReadTextFileResponse, RpcError> {
        absolute(&self.paths, &self.host, &request.path)?;
        let text = self
            .current_text(&self.paths.opened(&request.path))
            .map_err(|error| failure(&error, READ_FAILED_TEXT))?;
        Ok(ReadTextFileResponse {
            content: excerpt(text, request.line, request.limit),
        })
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn write(&self, request: WriteTextFileRequest) -> Result<EmptyResponse, RpcError> {
        absolute(&self.paths, &self.host, &request.path)?;
        let path = self.paths.opened(&request.path);
        let previous = match self.current_text(&path) {
            Ok(text) => text,
            Err(error) if error.is_not_found() => String::new(),
            Err(error) => return Err(failure(&error, WRITE_FAILED_TEXT)),
        };
        path.parent()
            .map_or(Ok(()), |parent| self.host.create_dir_all(parent))
            .and_then(|()| self.host.write(&path, request.content.as_bytes()))
            .map_err(|error| failure(&error, WRITE_FAILED_TEXT))?;
        self.buffers.written(&path, &request.content);
        let edit = FileEdit {
            path: self.paths.shown(&request.path),
            lines: line_diff(&previous, &request.content),
        };
        let written = FileWritten {
            session_id: request.session_id,
            path,
            edit,
        };
        if self.written.send(written).is_err() {
            debug!("the chat stopped listening to the files an ACP agent writes");
        }
        Ok(EmptyResponse {})
    }

    fn current_text(&self, path: &Path) -> Result<String, HostError> {
        if let Some(text) = self.buffers.unsaved_text(path) {
            return Ok(text);
        }
        self.host.read_to_string(path)
    }
}

fn failure(error: &HostError, context: &str) -> RpcError {
    let code = if error.is_not_found() {
        ErrorCode::RESOURCE_NOT_FOUND
    } else {
        ErrorCode::INTERNAL_ERROR
    };
    debug!(code = code.0, "an ACP file request failed");
    RpcError::new(code, format!("{context}: {error}"))
}

fn excerpt(text: String, line: Option<u32>, limit: Option<u32>) -> String {
    if line.is_none() && limit.is_none() {
        return text;
    }
    let skipped = line.map_or(0, |line| count(line.saturating_sub(FIRST_LINE)));
    let taken = limit.map_or(usize::MAX, count);
    text.split_inclusive(LINE_END)
        .skip(skipped)
        .take(taken)
        .collect()
}

fn count(lines: u32) -> usize {
    usize::try_from(lines).unwrap_or(usize::MAX)
}
