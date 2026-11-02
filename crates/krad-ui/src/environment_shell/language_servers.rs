mod diagnostics;
mod edits;
mod lifecycle;
mod notice;
mod providers;
mod refactor;
mod references;
mod toolchain_switch;
mod tooling;

use gpui_kit::component::input::EditorState;
use gpui_kit::{Context, Entity, Task};
use krad_domain::{
    EnvironmentName, Host, LanguageCatalog, LanguageDef, TerminalId, ToolchainLocation,
};
use krad_extensions::{builtin_catalog, LanguageTools, MissingServerRuntime, ServerPlan};
use krad_lsp::lsp_types::ServerCapabilities;
use krad_lsp::{DocumentChange, LanguageServer, LspClient};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{EnvironmentShell, LiveEnv};
pub(super) use diagnostics::severity_of;
use providers::ServerDocument;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::environment_shell) struct Document {
    pub(in crate::environment_shell) host: Host,
    pub(in crate::environment_shell) path: PathBuf,
}

impl Document {
    pub(in crate::environment_shell) fn new(host: &Host, path: &Path) -> Self {
        Self {
            host: host.clone(),
            path: path.to_path_buf(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ServerKey {
    host: krad_domain::Host,
    root: PathBuf,
    plan: ServerPlan,
    toolchain: Option<ToolchainLocation>,
}

pub(super) struct RunningServer {
    server: LanguageServer,
    pub(super) capabilities: ServerCapabilities,
    _events: Task<()>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct StartId(u64);

impl StartId {
    const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

enum ServerSlot {
    Locating,
    NeedsServerRuntime(MissingServerRuntime),
    InstallingServerRuntime(String),
    AwaitingConsent,
    Installing,
    Starting(StartId),
    Running(Box<RunningServer>),
    Failed(String),
}

pub(super) struct LanguageServers {
    tools: Option<Arc<LanguageTools>>,
    catalog: LanguageCatalog,
    slots: HashMap<ServerKey, ServerSlot>,
    declined: HashSet<String>,
    documents: HashMap<Document, ServerKey>,
    pub(super) diagnostics: diagnostics::DiagnosticStore,
    pending_edits: Vec<(Host, krad_lsp::lsp_types::WorkspaceEdit)>,
    last_start: StartId,
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
            last_start: StartId::default(),
        }
    }

    pub(super) fn running(&self, key: &ServerKey) -> Option<&RunningServer> {
        match self.slots.get(key)? {
            ServerSlot::Running(running) => Some(running),
            _ => None,
        }
    }

    pub(super) fn client_of(&self, document: &Document) -> Option<&LspClient> {
        let key = self.documents.get(document)?;
        self.running(key).map(|running| &running.server.client)
    }
}

impl EnvironmentShell {
    pub(super) fn language_tools(&self) -> Option<Arc<LanguageTools>> {
        self.language_servers.tools.clone()
    }

    pub(in crate::environment_shell) fn active_document(&self, path: &Path) -> Document {
        let host = self
            .envs
            .get(&self.active)
            .map_or_else(Host::default, |live| live.data.host.clone());
        Document::new(&host, path)
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
        if tools.server_toolchain(&plan).is_some() {
            return self.toolchain_server_key(&Document::new(&live.data.host, path), plan);
        }
        let root = live
            .data
            .group_of(id)
            .map(|group| group.root.clone())
            .or_else(|| path.parent().map(Path::to_path_buf))?;
        Some(ServerKey {
            host: live.data.host.clone(),
            root,
            plan,
            toolchain: None,
        })
    }

    pub(super) fn attach_language_server(
        &mut self,
        id: TerminalId,
        path: &Path,
        state: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let active = self.active.clone();
        self.attach_language_server_in(&active, id, path, state, cx);
    }

    pub(super) fn attach_language_server_in(
        &mut self,
        env: &EnvironmentName,
        id: TerminalId,
        path: &Path,
        state: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.envs.get(env).map(|live| live.data.host.clone()) else {
            return;
        };
        let document = Document::new(&host, path);
        if self.toolchains_unresolved(&document) {
            self.resolve_toolchains(document, cx);
            return;
        }
        let Some(key) = self
            .envs
            .get(env)
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
        let opened_document = Document::new(&key.host, path);
        let document = ServerDocument::new(
            running.server.client.clone(),
            &opened_document,
            &running.capabilities,
            cx.entity().downgrade(),
        );
        if let Some(document) = document {
            state.update(cx, |state, _| document.install(state.lsp_mut()));
        }
        if self
            .language_servers
            .documents
            .contains_key(&opened_document)
        {
            self.show_diagnostics(&opened_document, cx);
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
                .insert(opened_document.clone(), key.clone());
            self.pull_diagnostics(&opened_document, cx);
        }
        self.show_diagnostics(&opened_document, cx);
    }

    pub(super) fn document_edited(
        &mut self,
        document: &Document,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.language_servers.client_of(document) else {
            return;
        };
        client
            .document(DocumentChange::Edited {
                path: document.path.clone(),
                text,
            })
            .unwrap_or_default();
        self.pull_diagnostics(document, cx);
    }

    pub(super) fn document_saved(&self, document: &Document) {
        if let Some(client) = self.language_servers.client_of(document) {
            client
                .document(DocumentChange::Saved {
                    path: document.path.clone(),
                })
                .unwrap_or_default();
        }
    }

    pub(super) fn close_unseen_documents(&mut self) {
        let open: HashSet<Document> = self
            .envs
            .values()
            .flat_map(|live| {
                live.editors
                    .values()
                    .map(|editor| Document::new(&live.data.host, &editor.path))
            })
            .collect();
        self.toolchains.keep_documents(&open);
        let closed: Vec<Document> = self
            .language_servers
            .documents
            .keys()
            .filter(|document| !open.contains(*document))
            .cloned()
            .collect();
        for document in closed {
            if let Some(client) = self.language_servers.client_of(&document) {
                client
                    .document(DocumentChange::Closed {
                        path: document.path.clone(),
                    })
                    .unwrap_or_default();
            }
            self.language_servers.documents.remove(&document);
            self.language_servers.diagnostics.forget(&document);
        }
    }
}
