use gpui_kit::{AppContext as _, Context};
use krad_lsp::lsp_types::request::WorkspaceDiagnosticRequest;
use krad_lsp::lsp_types::{
    PartialResultParams, WorkDoneProgressParams, WorkspaceDiagnosticParams,
    WorkspaceDiagnosticReportResult, WorkspaceDocumentDiagnosticReport,
};
use krad_lsp::{uri_path, DocumentChange};
use std::path::PathBuf;
use std::time::Duration;

use super::super::super::EnvironmentShell;
use super::super::{Document, ServerKey};
use super::MAX_FILE_BYTES;

const BATCH_FILES: usize = 16;
const BATCH_PAUSE: Duration = Duration::from_millis(250);

impl EnvironmentShell {
    pub(super) fn pull_workspace(&self, key: &ServerKey, cx: &mut Context<Self>) {
        let Some(running) = self.language_servers.running(key) else {
            return;
        };
        let client = running.server.client.clone();
        let params = WorkspaceDiagnosticParams {
            identifier: None,
            previous_result_ids: Vec::new(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let host = key.host.clone();
        cx.spawn(async move |this, cx| {
            let Ok(WorkspaceDiagnosticReportResult::Report(report)) =
                client.request::<WorkspaceDiagnosticRequest>(&params).await
            else {
                return;
            };
            this.update(cx, |shell, cx| {
                for item in report.items {
                    let WorkspaceDocumentDiagnosticReport::Full(full) = item else {
                        continue;
                    };
                    let Some(path) = uri_path(&full.uri) else {
                        continue;
                    };
                    let document = Document::new(&host, &path);
                    shell
                        .language_servers
                        .diagnostics
                        .pulled(&document, full.full_document_diagnostic_report.items);
                    shell.show_diagnostics(&document, cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn open_project_files(&mut self, key: &ServerKey, cx: &mut Context<Self>) {
        let Some(check) = self.project_checks().servers.get(key) else {
            return;
        };
        let Some(files) = self.project_checks().files_of(check) else {
            return;
        };
        let wanted: Vec<PathBuf> = files
            .files
            .iter()
            .filter(|path| {
                !self
                    .language_servers
                    .documents
                    .contains_key(&Document::new(&key.host, path))
            })
            .cloned()
            .collect();
        let Some((link, _)) = self.server_link(&key.host) else {
            return;
        };
        let opened = key.clone();
        let task = cx.spawn(async move |this, cx| {
            for batch in wanted.chunks(BATCH_FILES) {
                let reading_link = link.clone();
                let paths = batch.to_vec();
                let texts = cx
                    .background_spawn(async move {
                        paths
                            .into_iter()
                            .filter_map(|path| {
                                let file =
                                    krad_repo::read_text(&reading_link, &path, MAX_FILE_BYTES)
                                        .ok()?;
                                (!file.truncated).then_some((path, file.text))
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                let going =
                    this.update(cx, |shell, cx| shell.open_in_background(&opened, texts, cx));
                if !matches!(going, Ok(true)) {
                    return;
                }
                cx.background_executor().timer(BATCH_PAUSE).await;
            }
        });
        if let Some(check) = self.language_servers.project.servers.get_mut(key) {
            check.opening = Some(task);
        }
    }

    pub(super) fn open_in_background(
        &mut self,
        key: &ServerKey,
        texts: Vec<(PathBuf, String)>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(running) = self.language_servers.running(key) else {
            return false;
        };
        let client = running.server.client.clone();
        for (path, text) in texts {
            let document = Document::new(&key.host, &path);
            if self.language_servers.documents.contains_key(&document) {
                continue;
            }
            let Some(language) = self.language_servers.catalog.for_path(&path) else {
                continue;
            };
            let opened = client.document(DocumentChange::Opened {
                path: path.clone(),
                language: language.lsp_id().to_owned(),
                text,
            });
            if opened.is_err() {
                return false;
            }
            self.language_servers
                .documents
                .insert(document.clone(), key.clone());
            self.language_servers
                .project
                .background
                .insert(document.clone());
            self.pull_diagnostics(&document, cx);
        }
        true
    }
}
