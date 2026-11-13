use gpui_kit::Context;
use krad_domain::Host;
use krad_extensions::ProjectCheck;
use krad_lsp::lsp_types::notification::DidChangeWatchedFiles;
use krad_lsp::lsp_types::{DidChangeWatchedFilesParams, FileEvent};
use krad_lsp::{file_uri, DocumentChange};
use krad_protocol::{Answer, FileAsk, TextRead};
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
        let Some(env) = self.env_on_host(&key.host) else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        let host = key.host.clone();
        let served = key.clone();
        let ask = FileAsk::ReadTexts {
            paths,
            limit: MAX_FILE_BYTES,
        };
        let reading = self.ask_file(&env, ask, texts_of, cx);
        cx.spawn(async move |this, cx| {
            let texts: Vec<(PathBuf, String)> = reading
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|read| (read.path, read.text))
                .collect();
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
        let host = self.active_host();
        let paths: Vec<PathBuf> = self
            .language_servers
            .diagnostics
            .files()
            .filter(|document| document.host == host)
            .map(|document| document.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }
        let checking = self.ask_active_file(
            FileAsk::Present {
                paths: paths.clone(),
            },
            present_of,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let Ok(present) = checking.await else {
                return;
            };
            let gone: Vec<(PathBuf, DiskChange)> = paths
                .into_iter()
                .zip(present)
                .filter(|(_, present)| !present)
                .map(|(path, _)| (path, DiskChange::Deleted))
                .collect();
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

pub(super) fn texts_of(answer: Answer) -> Option<Vec<TextRead>> {
    match answer {
        Answer::Texts { texts } => Some(texts),
        _ => None,
    }
}

fn present_of(answer: Answer) -> Option<Vec<bool>> {
    match answer {
        Answer::Present { present } => Some(present),
        _ => None,
    }
}
