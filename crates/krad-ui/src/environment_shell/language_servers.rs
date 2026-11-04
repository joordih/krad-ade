mod choice;
mod code_tools;
mod diagnostics;
mod edits;
mod file_templates;
mod input;
mod library_documents;
mod lifecycle;
mod notice;
mod progress;
mod project_check;
mod providers;
mod refactor;
mod references;
mod toolchain_switch;
mod tooling;
mod workspace_folders;

use gpui_kit::component::input::EditorState;
use gpui_kit::{Context, Entity, Task};
use krad_domain::{
    EnvironmentName, Host, LanguageCatalog, LanguageDef, ProblemSeverity, TerminalId,
    ToolchainLocation,
};
use krad_extensions::{
    builtin_catalog, LanguageTools, MissingServerRuntime, ServerPlan, ServerScope,
};
use krad_lsp::lsp_types::ServerCapabilities;
use krad_lsp::{DocumentChange, LanguageServer, LspClient};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{EnvironmentShell, LiveEnv};
pub(super) use choice::ChoiceAnswer;
pub(super) use code_tools::CodeCaller;
pub(super) use diagnostics::{problem_severity, severity_of};
pub(super) use file_templates::{ServerTemplates, TemplateOutput};
pub(super) use project_check::DiskChange;
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
    root: ServerRoot,
    plan: ServerPlan,
    toolchain: Option<ToolchainLocation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum ServerRoot {
    Folder(PathBuf),
    Host,
}

impl ServerRoot {
    pub(super) fn folder(&self) -> Option<&Path> {
        match self {
            Self::Folder(folder) => Some(folder),
            Self::Host => None,
        }
    }
}

pub(super) struct RunningServer {
    server: LanguageServer,
    pub(super) capabilities: ServerCapabilities,
    progress: progress::ServerProgress,
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
    Failed(ServerFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ServerFailure {
    summary: String,
    details: Option<String>,
}

impl ServerFailure {
    pub(super) fn reason(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            details: None,
        }
    }
}

impl From<&krad_extensions::ExtensionError> for ServerFailure {
    fn from(error: &krad_extensions::ExtensionError) -> Self {
        Self {
            summary: error.to_string(),
            details: error.details().map(str::to_owned),
        }
    }
}

pub(super) struct LanguageServers {
    tools: Option<Arc<LanguageTools>>,
    catalog: LanguageCatalog,
    slots: HashMap<ServerKey, ServerSlot>,
    declined: HashSet<String>,
    shown_details: HashSet<ServerKey>,
    documents: HashMap<Document, ServerKey>,
    folders: HashMap<ServerKey, BTreeSet<PathBuf>>,
    library: HashMap<Document, library_documents::LibraryDocument>,
    pub(super) diagnostics: diagnostics::DiagnosticStore,
    project: project_check::ProjectChecks,
    pending_edits: Vec<(Host, krad_lsp::lsp_types::WorkspaceEdit)>,
    pending_rename: Option<(Host, PathBuf, krad_lsp::lsp_types::Position)>,
    pending_choice: Option<(ServerKey, krad_lsp::ServerChoice)>,
    pending_input: Option<input::PendingInput>,
    pending_conflicts: Option<(ServerKey, krad_lsp::ServerConflicts)>,
    last_start: StartId,
}

impl LanguageServers {
    pub(super) fn new() -> Self {
        Self {
            tools: None,
            catalog: builtin_catalog(),
            slots: HashMap::new(),
            shown_details: HashSet::new(),
            declined: HashSet::new(),
            documents: HashMap::new(),
            folders: HashMap::new(),
            library: HashMap::new(),
            diagnostics: diagnostics::DiagnosticStore::default(),
            project: project_check::ProjectChecks::default(),
            pending_edits: Vec::new(),
            pending_rename: None,
            pending_choice: None,
            pending_input: None,
            pending_conflicts: None,
            last_start: StartId::default(),
        }
    }

    pub(super) fn running(&self, key: &ServerKey) -> Option<&RunningServer> {
        match self.slots.get(key)? {
            ServerSlot::Running(running) => Some(running),
            _ => None,
        }
    }

    pub(super) fn progress_label(&self, document: &Document) -> Option<String> {
        let key = self.documents.get(document)?;
        self.running(key)?.progress.label()
    }

    pub(super) fn client_of(&self, document: &Document) -> Option<&LspClient> {
        let key = self.documents.get(document)?;
        self.running(key).map(|running| &running.server.client)
    }
}

impl EnvironmentShell {
    pub(super) fn worst_problem_at(&self, path: &Path) -> Option<ProblemSeverity> {
        self.language_servers
            .diagnostics
            .worst_at(&self.active_document(path))
    }

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

    pub(super) const fn language_catalog(&self) -> &LanguageCatalog {
        &self.language_servers.catalog
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
        let root = match plan.scope {
            ServerScope::Workspace => ServerRoot::Host,
            ServerScope::Folder if tools.server_toolchain(&plan).is_some() => {
                return self.toolchain_server_key(&Document::new(&live.data.host, path), plan);
            }
            ServerScope::Folder => ServerRoot::Folder(Self::document_folder(live, id, path)?),
        };
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
        if let Some(library) = self.language_servers.library.get(&document).cloned() {
            self.open_document(&library.key, path, state, cx);
            return;
        }
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
        if key.root == ServerRoot::Host {
            if let Some(folder) = self
                .envs
                .get(env)
                .and_then(|live| Self::document_folder(live, id, path))
            {
                self.register_folder(&key, folder);
            }
        }
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
        let library = self
            .language_servers
            .library
            .get(&opened_document)
            .map(|library| library.uri.clone());
        let document = ServerDocument::new(
            running.server.client.clone(),
            &opened_document,
            library.clone(),
            &running.capabilities,
            cx.entity().downgrade(),
        );
        if let Some(document) = document {
            state.update(cx, |state, _| document.install(state.lsp_mut()));
        }
        if library.is_some() {
            return;
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
        let project = &self.language_servers.project;
        let open: HashSet<Document> = self
            .envs
            .values()
            .flat_map(|live| {
                live.editors
                    .values()
                    .map(|editor| Document::new(&live.data.host, &editor.path))
            })
            .chain(project.background.iter().cloned())
            .chain(project.representatives.iter().cloned())
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
            if self.keeps_for_project_check(&document) {
                continue;
            }
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
