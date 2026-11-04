use gpui_kit::{AppContext as _, Context};
use krad_domain::Host;
use krad_extensions::ProjectCheck;
use krad_lsp::lsp_types::notification::DidChangeWatchedFiles;
use krad_lsp::lsp_types::{DidChangeWatchedFilesParams, FileEvent};
use krad_lsp::{file_uri, DocumentChange};
use std::path::PathBuf;

use super::super::super::EnvironmentShell;
use super::super::{Document, ServerKey};
use super::{DiskChange, MAX_FILE_BYTES};

impl EnvironmentShell {
    pub(in crate::environment_shell) fn project_files_changed(
        &mut self,
        host: &Host,
        changed: &[(PathBuf, DiskChange)],
        cx: &mut Context<Self>,
    ) {
        let keys: Vec<ServerKey> = self
            .language_servers
            .slots
            .keys()
            .filter(|key| key.host == *host && self.language_servers.running(key).is_some())
            .cloned()
            .collect();
        for (path, change) in changed {
            if *change == DiskChange::Deleted {
                self.forget_deleted(&Document::new(host, path));
            }
        }
        for key in keys {
            let mine: Vec<(PathBuf, DiskChange)> = changed
                .iter()
                .filter(|(path, _)| key.root.folder().is_none_or(|root| path.starts_with(root)))
                .cloned()
                .collect();
            if !mine.is_empty() {
                self.server_files_changed(&key, &mine, cx);
            }
        }
    }

    fn server_files_changed(
        &mut self,
        key: &ServerKey,
        changed: &[(PathBuf, DiskChange)],
        cx: &mut Context<Self>,
    ) {
        let Some(running) = self.language_servers.running(key) else {
            return;
        };
        let client = running.server.client.clone();
        let events = changed
            .iter()
            .filter_map(|(path, change)| {
                Some(FileEvent::new(file_uri(path).ok()?, (*change).into()))
            })
            .collect();
        client
            .notify::<DidChangeWatchedFiles>(&DidChangeWatchedFilesParams { changes: events })
            .unwrap_or_default();
        let opens_files = self.server_config(key).project_check == ProjectCheck::OpenFiles;
        let mut reread = Vec::new();
        for (path, change) in changed {
            let document = Document::new(&key.host, path);
            let background = self.language_servers.project.background.contains(&document);
            match change {
                DiskChange::Deleted if background => self.close_background(&document),
                DiskChange::Deleted => self.language_servers.diagnostics.forget(&document),
                DiskChange::Created | DiskChange::Changed if background => {
                    reread.push(path.clone());
                }
                DiskChange::Created if opens_files && self.serves(key, path) => {
                    reread.push(path.clone());
                }
                DiskChange::Created | DiskChange::Changed => {}
            }
        }
        self.reread_background(key, reread, cx);
    }

    fn serves(&self, key: &ServerKey, path: &std::path::Path) -> bool {
        let servers = &self.language_servers;
        let plan = servers
            .tools
            .as_ref()
            .zip(servers.catalog.for_path(path))
            .and_then(|(tools, language)| tools.plan(language));
        plan.as_ref() == Some(&key.plan)
            && !servers
                .documents
                .contains_key(&Document::new(&key.host, path))
    }

    fn reread_background(&self, key: &ServerKey, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some((link, _)) = self.server_link(&key.host) else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        let host = key.host.clone();
        let served = key.clone();
        let reading = cx.background_spawn(async move {
            paths
                .into_iter()
                .filter_map(|path| {
                    let file = krad_repo::read_text(&link, &path, MAX_FILE_BYTES).ok()?;
                    Some((path, file.text))
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let texts = reading.await;
            this.update(cx, |shell, cx| {
                let mut fresh = Vec::new();
                for (path, text) in texts {
                    let document = Document::new(&host, &path);
                    if shell
                        .language_servers
                        .project
                        .background
                        .contains(&document)
                    {
                        shell.document_edited(&document, text, cx);
                    } else {
                        fresh.push((path, text));
                    }
                }
                shell.open_in_background(&served, fresh, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn forget_deleted(&mut self, document: &Document) {
        if self.language_servers.project.background.contains(document) {
            self.close_background(document);
        }
        self.language_servers.diagnostics.forget(document);
        self.language_servers
            .project
            .published
            .values_mut()
            .for_each(|published| {
                published.remove(document);
            });
    }

    pub(in crate::environment_shell) fn forget_missing_problems(&mut self, cx: &mut Context<Self>) {
        let link = self.active_link();
        let host = link.host();
        let paths: Vec<PathBuf> = self
            .language_servers
            .diagnostics
            .files()
            .into_iter()
            .filter(|document| document.host == host)
            .map(|document| document.path)
            .collect();
        if paths.is_empty() {
            return;
        }
        let checking = cx.background_spawn(async move {
            paths
                .into_iter()
                .filter(|path| !link.exists(path))
                .map(|path| (path, DiskChange::Deleted))
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let gone = checking.await;
            if gone.is_empty() {
                return;
            }
            this.update(cx, |shell, cx| {
                shell.project_files_changed(&host, &gone, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn close_background(&mut self, document: &Document) {
        if let Some(client) = self.language_servers.client_of(document) {
            client
                .document(DocumentChange::Closed {
                    path: document.path.clone(),
                })
                .unwrap_or_default();
        }
        self.language_servers.project.background.remove(document);
        self.language_servers.documents.remove(document);
        self.language_servers.diagnostics.forget(document);
    }

    pub(in crate::environment_shell) fn keeps_for_project_check(
        &mut self,
        document: &Document,
    ) -> bool {
        let Some(key) = self.language_servers.documents.get(document).cloned() else {
            return false;
        };
        if !self.language_servers.project.wants(&key) {
            return false;
        }
        if self.server_config(&key).project_check == ProjectCheck::OpenFiles {
            self.language_servers
                .project
                .background
                .insert(document.clone());
            return true;
        }
        if let Some(client) = self.language_servers.client_of(document) {
            client
                .document(DocumentChange::Closed {
                    path: document.path.clone(),
                })
                .unwrap_or_default();
        }
        self.language_servers.documents.remove(document);
        self.language_servers.diagnostics.forget_pulled(document);
        true
    }
}
