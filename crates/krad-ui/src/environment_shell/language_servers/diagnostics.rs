use gpui_kit::component::highlighter::Diagnostic;
use gpui_kit::component::input::{EditorState, RopeExt as _};
use gpui_kit::{Context, Entity, Task};
use krad_lsp::file_uri;
use krad_lsp::lsp_types::request::DocumentDiagnosticRequest;
use krad_lsp::lsp_types::{
    self, DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, PartialResultParams, TextDocumentIdentifier,
    WorkDoneProgressParams,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::EnvironmentShell;

const PULL_DELAY: Duration = Duration::from_millis(200);
const RETRY_DELAY: Duration = Duration::from_millis(800);
const PULL_ATTEMPTS: u32 = 5;

#[derive(Default)]
pub(in crate::environment_shell) struct DiagnosticStore {
    pushed: HashMap<PathBuf, Vec<lsp_types::Diagnostic>>,
    pulled: HashMap<PathBuf, Vec<lsp_types::Diagnostic>>,
    pulls: HashMap<PathBuf, Task<()>>,
}

impl DiagnosticStore {
    pub(super) fn pushed(&mut self, path: PathBuf, diagnostics: Vec<lsp_types::Diagnostic>) {
        self.pushed.insert(path, diagnostics);
    }

    fn pulled(&mut self, path: PathBuf, diagnostics: Vec<lsp_types::Diagnostic>) {
        self.pulled.insert(path, diagnostics);
    }

    pub(super) fn forget(&mut self, path: &Path) {
        self.pushed.remove(path);
        self.pulled.remove(path);
        self.pulls.remove(path);
    }

    pub(in crate::environment_shell) fn of(&self, path: &Path) -> Vec<lsp_types::Diagnostic> {
        let mut known: Vec<lsp_types::Diagnostic> = Vec::new();
        let sources = [self.pushed.get(path), self.pulled.get(path)];
        for diagnostic in sources.into_iter().flatten().flatten() {
            if !known.contains(diagnostic) {
                known.push(diagnostic.clone());
            }
        }
        known.sort_by_key(|diagnostic| {
            (
                diagnostic.range.start.line,
                diagnostic.range.start.character,
            )
        });
        known
    }

    pub(in crate::environment_shell) fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .pushed
            .keys()
            .chain(self.pulled.keys())
            .filter(|path| !self.of(path).is_empty())
            .cloned()
            .collect();
        files.sort();
        files.dedup();
        files
    }
}

pub(in crate::environment_shell) fn severity_of(
    diagnostic: &lsp_types::Diagnostic,
) -> DiagnosticSeverity {
    diagnostic.severity.unwrap_or(DiagnosticSeverity::ERROR)
}

fn editor_diagnostic(diagnostic: &lsp_types::Diagnostic) -> Diagnostic {
    let found = Diagnostic::new(
        diagnostic.range.start..diagnostic.range.end,
        diagnostic.message.clone(),
    )
    .with_severity(severity_of(diagnostic));
    match &diagnostic.source {
        Some(source) => found.with_source(source.clone()),
        None => found,
    }
}

fn report_items(report: DocumentDiagnosticReportResult) -> Option<Vec<lsp_types::Diagnostic>> {
    match report {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(full)) => {
            Some(full.full_document_diagnostic_report.items)
        }
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Unchanged(_))
        | DocumentDiagnosticReportResult::Partial(_) => None,
    }
}

impl EnvironmentShell {
    pub(super) fn pull_diagnostics(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.pull_diagnostics_after(path, PULL_DELAY, PULL_ATTEMPTS, cx);
    }

    fn pull_diagnostics_after(
        &mut self,
        path: &Path,
        delay: Duration,
        attempts: u32,
        cx: &mut Context<Self>,
    ) {
        let servers = &self.language_servers;
        let Some(key) = servers.documents.get(path) else {
            return;
        };
        let Some(running) = servers.running(key) else {
            return;
        };
        if running.capabilities.diagnostic_provider.is_none() {
            return;
        }
        let Ok(uri) = file_uri(path) else {
            return;
        };
        let client = running.server.client.clone();
        let params = DocumentDiagnosticParams {
            text_document: TextDocumentIdentifier::new(uri),
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let pulled = path.to_path_buf();
        let wait = cx.background_executor().timer(delay);
        let pulling = cx.spawn(async move |this, cx| {
            wait.await;
            let report = client.request::<DocumentDiagnosticRequest>(&params).await;
            let Ok(report) = report else {
                if let Some(left) = attempts.checked_sub(1).filter(|left| *left > 0) {
                    this.update(cx, |shell, cx| {
                        shell.pull_diagnostics_after(&pulled, RETRY_DELAY, left, cx);
                    })
                    .unwrap_or_default();
                }
                return;
            };
            let Some(items) = report_items(report) else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell
                    .language_servers
                    .diagnostics
                    .pulled(pulled.clone(), items);
                shell.show_diagnostics(&pulled, cx);
            })
            .unwrap_or_default();
        });
        self.language_servers
            .diagnostics
            .pulls
            .insert(path.to_path_buf(), pulling);
    }

    pub(in crate::environment_shell) fn show_diagnostics(
        &self,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        let known = self.language_servers.diagnostics.of(path);
        let states: Vec<Entity<EditorState>> = self
            .envs
            .values()
            .flat_map(|live| live.editors.values())
            .filter(|editor| editor.path == path)
            .filter_map(|editor| editor.state.clone())
            .collect();
        for state in states {
            state.update(cx, |state, cx| {
                let lines = state.text().lines_len();
                let found: Vec<Diagnostic> = known
                    .iter()
                    .filter(|diagnostic| (diagnostic.range.end.line as usize) < lines)
                    .map(editor_diagnostic)
                    .collect();
                if let Some(set) = state.diagnostics_mut() {
                    set.clear();
                    set.extend(found);
                }
                cx.notify();
            });
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::{Position, Range};

    fn at(line: u32, message: &str) -> lsp_types::Diagnostic {
        lsp_types::Diagnostic {
            range: Range::new(Position::new(line, 0), Position::new(line, 1)),
            message: message.to_owned(),
            ..lsp_types::Diagnostic::default()
        }
    }

    #[test]
    fn pushed_and_pulled_diagnostics_merge_once_in_line_order() {
        let mut store = DiagnosticStore::default();
        let file = PathBuf::from("/work/main.rs");
        store.pushed(file.clone(), vec![at(4, "from check"), at(1, "both")]);
        store.pulled(file.clone(), vec![at(1, "both"), at(2, "live")]);
        let messages: Vec<String> = store
            .of(&file)
            .into_iter()
            .map(|found| found.message)
            .collect();
        assert_eq!(messages, ["both", "live", "from check"]);
        assert_eq!(store.files(), std::slice::from_ref(&file));
        store.forget(&file);
        assert_eq!(store.files(), Vec::<PathBuf>::new());
    }
}
