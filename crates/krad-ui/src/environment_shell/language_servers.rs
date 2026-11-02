mod diagnostics;
mod edits;
mod lifecycle;
mod notice;
mod providers;
mod refactor;
mod references;
mod tooling;

use gpui_kit::component::input::EditorState;
use gpui_kit::{Context, Entity, Task};
use krad_domain::{LanguageCatalog, LanguageDef, TerminalId};
use krad_extensions::{builtin_catalog, LanguageTools, MissingToolchain, ServerPlan};
use krad_lsp::lsp_types::ServerCapabilities;
use krad_lsp::{DocumentChange, LanguageServer, LspClient};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{EnvironmentShell, LiveEnv};
pub(super) use diagnostics::severity_of;
use providers::ServerDocument;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ServerKey {
    root: PathBuf,
    plan: ServerPlan,
}

pub(super) struct RunningServer {
    server: LanguageServer,
    pub(super) capabilities: ServerCapabilities,
    _events: Task<()>,
}

enum ServerSlot {
    Locating,
    NeedsToolchain(MissingToolchain),
    InstallingToolchain(String),
    AwaitingConsent,
    Installing,
    Starting,
    Running(Box<RunningServer>),
    Failed(String),
}

pub(super) struct LanguageServers {
    tools: Option<Arc<LanguageTools>>,
    catalog: LanguageCatalog,
    slots: HashMap<ServerKey, ServerSlot>,
    declined: HashSet<String>,
    documents: HashMap<PathBuf, ServerKey>,
    pub(super) diagnostics: diagnostics::DiagnosticStore,
    pending_edits: Vec<krad_lsp::lsp_types::WorkspaceEdit>,
}

impl LanguageServers {
    pub(super) fn new() -> Self {
        Self {
            tools: None,
            catalog: builtin_catalog(),
            slots: HashMap::new(),
            declined: HashSet::new(),
            documents: HashMap::new(),
            diagnostics: diagnostics::DiagnosticStore::default(),
            pending_edits: Vec::new(),
        }
    }

    pub(super) fn running(&self, key: &ServerKey) -> Option<&RunningServer> {
        match self.slots.get(key)? {
            ServerSlot::Running(running) => Some(running),
            _ => None,
        }
    }

    pub(super) fn client_of(&self, path: &Path) -> Option<&LspClient> {
        let key = self.documents.get(path)?;
        self.running(key).map(|running| &running.server.client)
    }
}

impl EnvironmentShell {
    pub(super) fn language_tools(&self) -> Option<Arc<LanguageTools>> {
        self.language_servers.tools.clone()
    }

    pub(super) fn language_of(&self, path: &Path) -> Option<LanguageDef> {
        self.language_servers.catalog.for_path(path).cloned()
    }

    pub(super) fn server_key(
        &self,
        live: &LiveEnv,
        id: TerminalId,
        path: &Path,
    ) -> Option<ServerKey> {
        let tools = self.language_servers.tools.as_ref()?;
        let plan = tools.plan(self.language_servers.catalog.for_path(path)?)?;
        let root = live
            .data
            .group_of(id)
            .map(|group| group.root.clone())
            .or_else(|| path.parent().map(Path::to_path_buf))?;
        Some(ServerKey { root, plan })
    }

    pub(super) fn attach_language_server(
        &mut self,
        id: TerminalId,
        path: &Path,
        state: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self
            .envs
            .get(&self.active)
            .and_then(|live| self.server_key(live, id, path))
        else {
            return;
        };
        if self.language_servers.running(&key).is_some() {
            self.open_document(&key, path, state, cx);
            return;
        }
        self.ensure_server(key, cx);
    }

    pub(super) fn open_document(
        &mut self,
        key: &ServerKey,
        path: &Path,
        state: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let Some(running) = self.language_servers.running(key) else {
            return;
        };
        let document = ServerDocument::new(
            running.server.client.clone(),
            path,
            &running.capabilities,
            cx.entity().downgrade(),
        );
        if let Some(document) = document {
            state.update(cx, |state, _| document.install(state.lsp_mut()));
        }
        if self.language_servers.documents.contains_key(path) {
            self.show_diagnostics(path, cx);
            return;
        }
        let Some(language) = self.language_servers.catalog.for_path(path) else {
            return;
        };
        let opened = running.server.client.document(DocumentChange::Opened {
            path: path.to_path_buf(),
            language: language.lsp_id().to_owned(),
            text: state.read(cx).value().to_string(),
        });
        if opened.is_ok() {
            self.language_servers
                .documents
                .insert(path.to_path_buf(), key.clone());
            self.pull_diagnostics(path, cx);
        }
        self.show_diagnostics(path, cx);
    }

    pub(super) fn document_edited(&mut self, path: &Path, text: String, cx: &mut Context<Self>) {
        let Some(client) = self.language_servers.client_of(path) else {
            return;
        };
        client
            .document(DocumentChange::Edited {
                path: path.to_path_buf(),
                text,
            })
            .unwrap_or_default();
        self.pull_diagnostics(path, cx);
    }

    pub(super) fn document_saved(&self, path: &Path) {
        if let Some(client) = self.language_servers.client_of(path) {
            client
                .document(DocumentChange::Saved {
                    path: path.to_path_buf(),
                })
                .unwrap_or_default();
        }
    }

    pub(super) fn close_unseen_documents(&mut self) {
        let open: HashSet<&Path> = self
            .envs
            .values()
            .flat_map(|live| live.editors.values())
            .map(|editor| editor.path.as_path())
            .collect();
        let closed: Vec<PathBuf> = self
            .language_servers
            .documents
            .keys()
            .filter(|path| !open.contains(path.as_path()))
            .cloned()
            .collect();
        for path in closed {
            if let Some(client) = self.language_servers.client_of(&path) {
                client
                    .document(DocumentChange::Closed { path: path.clone() })
                    .unwrap_or_default();
            }
            self.language_servers.documents.remove(&path);
            self.language_servers.diagnostics.forget(&path);
        }
    }
}
