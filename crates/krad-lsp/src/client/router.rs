use futures::channel::{mpsc, oneshot};
use futures::executor::block_on;
use futures::SinkExt as _;
use lsp_types::notification::{
    DidChangeConfiguration, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Notification, Progress, PublishDiagnostics,
};
use lsp_types::request::{ApplyWorkspaceEdit, Request as _, WorkspaceDiagnosticRefresh};
use lsp_types::{
    ApplyWorkspaceEditParams, ApplyWorkspaceEditResponse, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, ProgressParams, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc::Receiver;

use super::{DocumentChange, ServerEvent, ServerSettings};
use crate::error::LspError;
use crate::rpc::{write_message, Incoming, Message};
use crate::uri::file_uri;

const FIRST_VERSION: i32 = 0;

pub(super) enum Routed {
    Request {
        id: i64,
        method: &'static str,
        params: Value,
        reply: oneshot::Sender<Result<Value, LspError>>,
    },
    Notify {
        method: &'static str,
        params: Value,
    },
    Document(DocumentChange),
    Settings(ServerSettings),
    Incoming(Incoming),
    Closed,
}

pub(super) struct Router<W: Write> {
    input: W,
    events: mpsc::Sender<ServerEvent>,
    pending: HashMap<i64, oneshot::Sender<Result<Value, LspError>>>,
    versions: HashMap<Uri, i32>,
    settings: ServerSettings,
}

impl<W: Write> Router<W> {
    pub(super) fn new(
        input: W,
        events: mpsc::Sender<ServerEvent>,
        settings: ServerSettings,
    ) -> Self {
        Self {
            input,
            events,
            pending: HashMap::new(),
            versions: HashMap::new(),
            settings,
        }
    }

    pub(super) fn run(mut self, queue: &Receiver<Routed>) {
        while let Ok(routed) = queue.recv() {
            if !self.route(routed) {
                break;
            }
        }
        for (_, reply) in self.pending.drain() {
            reply.send(Err(LspError::Stopped)).unwrap_or_default();
        }
        self.emit(ServerEvent::Stopped);
    }

    fn route(&mut self, routed: Routed) -> bool {
        match routed {
            Routed::Request {
                id,
                method,
                params,
                reply,
            } => {
                let sent = write_message(
                    &mut self.input,
                    Message::Request {
                        id: &Value::from(id),
                        method,
                        params: &params,
                    },
                );
                if sent.is_err() {
                    reply.send(Err(LspError::Stopped)).unwrap_or_default();
                    return false;
                }
                self.pending.insert(id, reply);
            }
            Routed::Notify { method, params } => {
                if self.send(method, &params).is_err() {
                    return false;
                }
            }
            Routed::Document(change) => {
                if self.document(change).is_err() {
                    return false;
                }
            }
            Routed::Settings(settings) => {
                self.settings = settings;
                let changed = DidChangeConfigurationParams {
                    settings: self.settings.all(),
                };
                if self.notify::<DidChangeConfiguration>(&changed).is_err() {
                    return false;
                }
            }
            Routed::Incoming(message) => self.receive(message),
            Routed::Closed => return false,
        }
        true
    }

    fn send(&mut self, method: &str, params: &Value) -> Result<(), LspError> {
        write_message(&mut self.input, Message::Notification { method, params })
    }

    fn notify<N: Notification>(&mut self, params: &N::Params) -> Result<(), LspError> {
        let params = serde_json::to_value(params)?;
        self.send(N::METHOD, &params)
    }

    fn document(&mut self, change: DocumentChange) -> Result<(), LspError> {
        match change {
            DocumentChange::Opened {
                path,
                language,
                text,
            } => {
                let uri = file_uri(&path)?;
                self.versions.insert(uri.clone(), FIRST_VERSION);
                self.notify::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(uri, language, FIRST_VERSION, text),
                })
            }
            DocumentChange::Edited { path, text } => {
                let uri = file_uri(&path)?;
                let Some(version) = self.versions.get_mut(&uri) else {
                    return Ok(());
                };
                *version += 1;
                let version = *version;
                self.notify::<DidChangeTextDocument>(&DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier::new(uri, version),
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text,
                    }],
                })
            }
            DocumentChange::Saved { path } => {
                let uri = file_uri(&path)?;
                self.notify::<DidSaveTextDocument>(&DidSaveTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(uri),
                    text: None,
                })
            }
            DocumentChange::Closed { path } => {
                let uri = file_uri(&path)?;
                if self.versions.remove(&uri).is_none() {
                    return Ok(());
                }
                self.notify::<DidCloseTextDocument>(&DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(uri),
                })
            }
        }
    }

    fn receive(&mut self, message: Incoming) {
        match message {
            Incoming::Response { id, outcome } => {
                let Some(reply) = self.pending.remove(&id) else {
                    return;
                };
                let outcome = outcome.map_err(|error| LspError::Server {
                    code: error.code,
                    message: error.message,
                });
                reply.send(outcome).unwrap_or_default();
            }
            Incoming::Request { id, method, params } => {
                let result = if method == ApplyWorkspaceEdit::METHOD {
                    self.apply_edit(params)
                } else if method == WorkspaceDiagnosticRefresh::METHOD {
                    self.emit(ServerEvent::DiagnosticsRefresh);
                    Value::Null
                } else {
                    self.settings.reply(&method, params)
                };
                if write_message(
                    &mut self.input,
                    Message::Reply {
                        id: &id,
                        result: &result,
                    },
                )
                .is_err()
                {
                    self.emit(ServerEvent::Stopped);
                }
            }
            Incoming::Notification { method, params } => match method.as_str() {
                PublishDiagnostics::METHOD => {
                    if let Ok(diagnostics) = serde_json::from_value(params) {
                        self.emit(ServerEvent::Diagnostics(diagnostics));
                    }
                }
                Progress::METHOD => {
                    if let Ok(progress) = serde_json::from_value::<ProgressParams>(params) {
                        self.emit(ServerEvent::Progress(progress.into()));
                    }
                }
                _ => {}
            },
        }
    }

    fn apply_edit(&mut self, params: Value) -> Value {
        let Ok(asked) = serde_json::from_value::<ApplyWorkspaceEditParams>(params) else {
            return Value::Null;
        };
        self.emit(ServerEvent::ApplyEdit(asked.edit));
        serde_json::to_value(ApplyWorkspaceEditResponse {
            applied: true,
            failure_reason: None,
            failed_change: None,
        })
        .unwrap_or(Value::Null)
    }

    fn emit(&mut self, event: ServerEvent) {
        block_on(self.events.send(event)).unwrap_or_default();
    }
}
