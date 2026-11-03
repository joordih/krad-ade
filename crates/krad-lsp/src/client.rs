mod choice;
mod progress;
mod router;

pub use choice::{ChoiceId, ServerChoice};
pub use progress::{Progress, ProgressState};

use futures::channel::{mpsc, oneshot};
use lsp_types::notification::{DidChangeWorkspaceFolders, Notification};
use lsp_types::request::{Request, WorkspaceConfiguration};
use lsp_types::{
    ConfigurationParams, DidChangeWorkspaceFoldersParams, PublishDiagnosticsParams, WorkspaceEdit,
    WorkspaceFoldersChangeEvent,
};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;

use crate::error::LspError;
use crate::rpc::read_message;
use crate::uri::workspace_folder;
use router::{Routed, Router};

const ROUTER_QUEUE: usize = 256;
const EVENT_QUEUE: usize = 64;
const READER_THREAD: &str = "krad-lsp-reader";
const ROUTER_THREAD: &str = "krad-lsp-router";

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    Diagnostics(PublishDiagnosticsParams),
    ApplyEdit(WorkspaceEdit),
    DiagnosticsRefresh,
    Progress(Progress),
    Choice(ServerChoice),
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocumentChange {
    Opened {
        path: PathBuf,
        language: String,
        text: String,
    },
    Edited {
        path: PathBuf,
        text: String,
    },
    Saved {
        path: PathBuf,
    },
    Closed {
        path: PathBuf,
    },
}

#[derive(Clone)]
pub struct LspClient {
    router: SyncSender<Routed>,
    next_id: Arc<AtomicI64>,
}

impl LspClient {
    /// # Errors
    ///
    /// Returns [`LspError::Spawn`] when a pipe thread cannot start.
    pub fn over(
        output: impl BufRead + Send + 'static,
        input: impl Write + Send + 'static,
        settings: ServerSettings,
    ) -> Result<(Self, mpsc::Receiver<ServerEvent>), LspError> {
        let (router_tx, router_rx) = sync_channel(ROUTER_QUEUE);
        let (events_tx, events_rx) = mpsc::channel(EVENT_QUEUE);
        let reader = router_tx.clone();
        thread::Builder::new()
            .name(READER_THREAD.to_owned())
            .spawn(move || read_into(output, &reader))
            .map_err(LspError::Spawn)?;
        thread::Builder::new()
            .name(ROUTER_THREAD.to_owned())
            .spawn(move || Router::new(input, events_tx, settings).run(&router_rx))
            .map_err(LspError::Spawn)?;
        let client = Self {
            router: router_tx,
            next_id: Arc::new(AtomicI64::new(1)),
        };
        Ok((client, events_rx))
    }

    fn route(&self, routed: Routed) -> Result<(), LspError> {
        self.router.try_send(routed).map_err(|error| match error {
            TrySendError::Full(_) => LspError::Busy,
            TrySendError::Disconnected(_) => LspError::Stopped,
        })
    }

    /// # Errors
    ///
    /// The future fails with [`LspError`] when the server stops, refuses or answers badly.
    pub fn request<R: Request>(
        &self,
        params: &R::Params,
    ) -> impl std::future::Future<Output = Result<R::Result, LspError>> + use<R> {
        let sent = serde_json::to_value(params)
            .map_err(LspError::from)
            .and_then(|params| {
                let (reply, answer) = oneshot::channel();
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);
                self.route(Routed::Request {
                    id,
                    method: R::METHOD,
                    params,
                    reply,
                })
                .map(|()| answer)
            });
        async move {
            let value = sent?.await.map_err(|_| LspError::Stopped)??;
            Ok(serde_json::from_value(value)?)
        }
    }

    /// # Errors
    ///
    /// Returns [`LspError::Stopped`] or [`LspError::Busy`] when the server cannot take it.
    pub fn notify<N: Notification>(&self, params: &N::Params) -> Result<(), LspError> {
        let params = serde_json::to_value(params)?;
        self.route(Routed::Notify {
            method: N::METHOD,
            params,
        })
    }

    /// # Errors
    ///
    /// Returns [`LspError::Stopped`] or [`LspError::Busy`] when the server cannot take it.
    pub fn document(&self, change: DocumentChange) -> Result<(), LspError> {
        self.route(Routed::Document(change))
    }

    /// # Errors
    ///
    /// Returns [`LspError::Stopped`] or [`LspError::Busy`] when the server cannot take it, and
    /// [`LspError::Uri`] when a folder has no `file://` address.
    pub fn change_workspace_folders(
        &self,
        added: &[PathBuf],
        removed: &[PathBuf],
    ) -> Result<(), LspError> {
        let event = WorkspaceFoldersChangeEvent {
            added: added
                .iter()
                .map(|folder| workspace_folder(folder))
                .collect::<Result<_, _>>()?,
            removed: removed
                .iter()
                .map(|folder| workspace_folder(folder))
                .collect::<Result<_, _>>()?,
        };
        self.notify::<DidChangeWorkspaceFolders>(&DidChangeWorkspaceFoldersParams { event })
    }

    /// # Errors
    ///
    /// Returns [`LspError::Stopped`] or [`LspError::Busy`] when the server cannot take it.
    pub fn answer_choice(&self, id: ChoiceId, picked: Option<&str>) -> Result<(), LspError> {
        let result = picked.map_or(Value::Null, |title| serde_json::json!({ "title": title }));
        self.route(Routed::Answer { id: id.0, result })
    }

    /// # Errors
    ///
    /// Returns [`LspError::Stopped`] or [`LspError::Busy`] when the server cannot take it.
    pub fn reconfigure(&self, settings: ServerSettings) -> Result<(), LspError> {
        self.route(Routed::Settings(settings))
    }
}

fn read_into(mut output: impl BufRead, router: &SyncSender<Routed>) {
    loop {
        match read_message(&mut output) {
            Ok(Some(message)) => {
                if router.send(Routed::Incoming(message)).is_err() {
                    return;
                }
            }
            Ok(None) | Err(LspError::Pipe(_)) => break,
            Err(_) => {}
        }
    }
    router.send(Routed::Closed).unwrap_or_default();
}

const SECTION_SEPARATOR: char = '.';

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerSettings(Value);

impl ServerSettings {
    #[must_use]
    pub fn new(sections: Value) -> Self {
        Self(sections)
    }

    pub(crate) fn all(&self) -> Value {
        self.0.clone()
    }

    fn section(&self, name: Option<&str>) -> Value {
        let Some(name) = name else {
            return self.0.clone();
        };
        name.split(SECTION_SEPARATOR)
            .try_fold(&self.0, |value, key| value.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    }

    pub(crate) fn reply(&self, method: &str, params: Value) -> Value {
        if method != WorkspaceConfiguration::METHOD {
            return Value::Null;
        }
        let Ok(asked) = serde_json::from_value::<ConfigurationParams>(params) else {
            return Value::Array(Vec::new());
        };
        Value::Array(
            asked
                .items
                .iter()
                .map(|item| self.section(item.section.as_deref()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests;
