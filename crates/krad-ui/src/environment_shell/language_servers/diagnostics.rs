use gpui_kit::component::input::EditorState;
use gpui_kit::{Context, Entity, Task};
use krad_domain::ProblemSeverity;
use krad_lsp::file_uri;
use krad_lsp::lsp_types::request::DocumentDiagnosticRequest;
use krad_lsp::lsp_types::{
    self, DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, PartialResultParams, TextDocumentIdentifier,
    WorkDoneProgressParams,
};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::time::Duration;

mod problem_rows;

use problem_rows::ProblemRows;
pub(in crate::environment_shell) use problem_rows::{
    FileRow, HostProblems, ProblemLine, ProblemRow,
};

use super::super::file_tab::problem_list::ProblemList;
use super::super::EnvironmentShell;
use super::Document;

const PULL_DELAY: Duration = Duration::from_millis(200);
const RETRY_DELAY: Duration = Duration::from_millis(800);
const PULL_ATTEMPTS: u32 = 5;
const PROBLEMS_REDRAW: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(in crate::environment_shell) struct DiagnosticStore {
    pushed: HashMap<Document, Vec<lsp_types::Diagnostic>>,
    pulled: HashMap<Document, Vec<lsp_types::Diagnostic>>,
    pulls: HashMap<Document, Task<()>>,
    merged: HashMap<Document, Vec<lsp_types::Diagnostic>>,
    file_worst: HashMap<Document, ProblemSeverity>,
    places: HashMap<Document, SeverityCounts>,
    rows: OnceCell<ProblemRows>,
    redraw: Option<Task<()>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SeverityCounts([u32; 4]);

impl SeverityCounts {
    const ORDER: [ProblemSeverity; 4] = [
        ProblemSeverity::Error,
        ProblemSeverity::Warning,
        ProblemSeverity::Info,
        ProblemSeverity::Hint,
    ];

    const fn slot(severity: ProblemSeverity) -> usize {
        match severity {
            ProblemSeverity::Error => 0,
            ProblemSeverity::Warning => 1,
            ProblemSeverity::Info => 2,
            ProblemSeverity::Hint => 3,
        }
    }

    fn add(&mut self, severity: ProblemSeverity) {
        self.0[Self::slot(severity)] += 1;
    }

    fn remove(&mut self, severity: ProblemSeverity) {
        let count = &mut self.0[Self::slot(severity)];
        *count = count.saturating_sub(1);
    }

    fn worst(self) -> Option<ProblemSeverity> {
        Self::ORDER
            .into_iter()
            .find(|severity| self.0[Self::slot(*severity)] > 0)
    }
}

impl DiagnosticStore {
    #[must_use]
    pub(super) fn pushed(
        &mut self,
        document: &Document,
        diagnostics: Vec<lsp_types::Diagnostic>,
    ) -> bool {
        if self.pushed.get(document) == Some(&diagnostics) {
            return false;
        }
        self.pushed.insert(document.clone(), diagnostics);
        self.settle(document)
    }

    #[must_use]
    pub(super) fn pulled(
        &mut self,
        document: &Document,
        diagnostics: Vec<lsp_types::Diagnostic>,
    ) -> bool {
        if self.pulled.get(document) == Some(&diagnostics) {
            return false;
        }
        self.pulled.insert(document.clone(), diagnostics);
        self.settle(document)
    }

    pub(super) fn forget(&mut self, document: &Document) {
        self.pushed.remove(document);
        self.pulled.remove(document);
        self.pulls.remove(document);
        self.settle(document);
    }

    pub(super) fn forget_pulled(&mut self, document: &Document) {
        self.pulled.remove(document);
        self.pulls.remove(document);
        self.settle(document);
    }

    pub(in crate::environment_shell) fn worst_at(
        &self,
        place: &Document,
    ) -> Option<ProblemSeverity> {
        self.places.get(place).and_then(|counts| counts.worst())
    }

    fn settle(&mut self, document: &Document) -> bool {
        let sources = [self.pushed.get(document), self.pulled.get(document)];
        let merged = merge(sources.into_iter().flatten().flatten());
        if self.of(document) == merged.as_slice() {
            return false;
        }
        self.rows.take();
        let now = merged.iter().map(problem_severity).min();
        if merged.is_empty() {
            self.merged.remove(document);
        } else {
            self.merged.insert(document.clone(), merged);
        }
        let before = match now {
            Some(severity) => self.file_worst.insert(document.clone(), severity),
            None => self.file_worst.remove(document),
        };
        if before != now {
            self.settle_places(document, before, now);
        }
        true
    }

    fn settle_places(
        &mut self,
        document: &Document,
        before: Option<ProblemSeverity>,
        now: Option<ProblemSeverity>,
    ) {
        for place in document.path.ancestors() {
            let place = Document::new(&document.host, place);
            let counts = self.places.entry(place.clone()).or_default();
            if let Some(before) = before {
                counts.remove(before);
            }
            if let Some(now) = now {
                counts.add(now);
            }
            if counts.worst().is_none() {
                self.places.remove(&place);
            }
        }
    }

    pub(in crate::environment_shell) fn of(&self, document: &Document) -> &[lsp_types::Diagnostic] {
        self.merged.get(document).map_or(&[], Vec::as_slice)
    }

    pub(in crate::environment_shell) fn files(&self) -> impl Iterator<Item = &Document> {
        self.merged.keys()
    }

    pub(in crate::environment_shell) fn rows(&self) -> &ProblemRows {
        self.rows.get_or_init(|| ProblemRows::new(&self.merged))
    }
}

fn start_of(diagnostic: &lsp_types::Diagnostic) -> (u32, u32) {
    (
        diagnostic.range.start.line,
        diagnostic.range.start.character,
    )
}

fn merge<'a>(
    sources: impl Iterator<Item = &'a lsp_types::Diagnostic>,
) -> Vec<lsp_types::Diagnostic> {
    let mut all: Vec<&lsp_types::Diagnostic> = sources.collect();
    all.sort_by_key(|diagnostic| start_of(diagnostic));
    let mut known: Vec<lsp_types::Diagnostic> = Vec::with_capacity(all.len());
    let mut same_start = 0;
    for diagnostic in all {
        if known
            .last()
            .is_some_and(|last| start_of(last) != start_of(diagnostic))
        {
            same_start = known.len();
        }
        if !known[same_start..].contains(diagnostic) {
            known.push(diagnostic.clone());
        }
    }
    known
}

pub(in crate::environment_shell) fn severity_of(
    diagnostic: &lsp_types::Diagnostic,
) -> DiagnosticSeverity {
    diagnostic.severity.unwrap_or(DiagnosticSeverity::ERROR)
}

pub(in crate::environment_shell) fn problem_severity(
    diagnostic: &lsp_types::Diagnostic,
) -> ProblemSeverity {
    match severity_of(diagnostic) {
        DiagnosticSeverity::WARNING => ProblemSeverity::Warning,
        DiagnosticSeverity::INFORMATION => ProblemSeverity::Info,
        DiagnosticSeverity::HINT => ProblemSeverity::Hint,
        _ => ProblemSeverity::Error,
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
    pub(super) fn pull_diagnostics(&mut self, document: &Document, cx: &mut Context<Self>) {
        self.pull_diagnostics_after(document, PULL_DELAY, PULL_ATTEMPTS, cx);
    }

    fn pull_diagnostics_after(
        &mut self,
        document: &Document,
        delay: Duration,
        attempts: u32,
        cx: &mut Context<Self>,
    ) {
        let servers = &self.language_servers;
        let Some(key) = servers.documents.get(document) else {
            return;
        };
        let Some(running) = servers.running(key) else {
            return;
        };
        if running.capabilities.diagnostic_provider.is_none() {
            return;
        }
        let Ok(uri) = file_uri(&document.path) else {
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
        let pulled = document.clone();
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
                if shell.language_servers.diagnostics.pulled(&pulled, items) {
                    shell.show_diagnostics(&pulled, cx);
                }
            })
            .unwrap_or_default();
        });
        self.language_servers
            .diagnostics
            .pulls
            .insert(document.clone(), pulling);
    }

    pub(in crate::environment_shell) fn show_diagnostics(
        &mut self,
        document: &Document,
        cx: &mut Context<Self>,
    ) {
        let known = self.language_servers.diagnostics.of(document);
        let editors: Vec<(Entity<EditorState>, Entity<ProblemList>)> = self
            .envs
            .values()
            .filter(|live| live.data.host == document.host)
            .flat_map(|live| live.editors.values())
            .filter(|editor| editor.path == document.path)
            .filter_map(|editor| Some((editor.state.clone()?, editor.problems.clone()?)))
            .collect();
        for (state, problems) in editors {
            let found = ProblemList::from_diagnostics(known, state.read(cx).text());
            problems.update(cx, |problems, cx| {
                *problems = found;
                cx.notify();
            });
            state.update(cx, |state, cx| {
                if let Some(set) = state.diagnostics_mut() {
                    set.clear();
                }
                cx.notify();
            });
        }
        self.redraw_problems(cx);
    }

    fn redraw_problems(&mut self, cx: &mut Context<Self>) {
        let diagnostics = &mut self.language_servers.diagnostics;
        if diagnostics.redraw.is_some() {
            return;
        }
        let wait = cx.background_executor().timer(PROBLEMS_REDRAW);
        diagnostics.redraw = Some(cx.spawn(async move |this, cx| {
            wait.await;
            this.update(cx, |shell, cx| {
                shell.language_servers.diagnostics.redraw = None;
                cx.notify();
            })
            .unwrap_or_default();
        }));
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
        let file = Document::new(
            &krad_domain::Host::Local,
            std::path::Path::new("/work/main.rs"),
        );
        assert!(store.pushed(&file, vec![at(4, "from check"), at(1, "both")]));
        assert!(store.pulled(&file, vec![at(1, "both"), at(2, "live")]));
        let messages: Vec<&str> = store
            .of(&file)
            .iter()
            .map(|found| found.message.as_str())
            .collect();
        assert_eq!(messages, ["both", "live", "from check"]);
        let shown = store.rows().of(&krad_domain::Host::Local);
        assert_eq!(shown.map(|problems| problems.rows.len()), Some(4));
        store.forget(&file);
        assert!(store.rows().of(&krad_domain::Host::Local).is_none());
    }

    #[test]
    fn the_same_diagnostics_again_change_nothing() {
        let mut store = DiagnosticStore::default();
        let file = Document::new(
            &krad_domain::Host::Local,
            std::path::Path::new("/work/main.rs"),
        );
        assert!(store.pushed(&file, vec![at(1, "first")]));
        assert!(!store.pushed(&file, vec![at(1, "first")]));
        assert!(!store.pulled(&file, vec![at(1, "first")]));
        assert!(store.pulled(&file, vec![at(1, "first"), at(1, "second")]));
        assert!(!store.pushed(&file, Vec::new()));
        assert_eq!(store.of(&file).len(), 2);
    }

    #[test]
    fn folders_take_the_worst_problem_of_the_files_inside_them_as_problems_change() {
        let mut store = DiagnosticStore::default();
        let place =
            |path: &str| Document::new(&krad_domain::Host::Local, std::path::Path::new(path));
        let with = |severity: DiagnosticSeverity| lsp_types::Diagnostic {
            severity: Some(severity),
            ..at(0, "problem")
        };
        assert!(store.pushed(
            &place("/repo/src/a.rs"),
            vec![with(DiagnosticSeverity::WARNING)],
        ));
        assert!(store.pushed(
            &place("/repo/src/b.rs"),
            vec![with(DiagnosticSeverity::ERROR)],
        ));
        assert!(store.pushed(
            &place("/repo/docs/c.rs"),
            vec![with(DiagnosticSeverity::HINT)],
        ));
        assert!(!store.pushed(&place("/repo/clean.rs"), Vec::new()));
        assert_eq!(
            store.worst_at(&place("/repo/src/a.rs")),
            Some(ProblemSeverity::Warning)
        );
        assert_eq!(
            store.worst_at(&place("/repo/src")),
            Some(ProblemSeverity::Error)
        );
        assert_eq!(
            store.worst_at(&place("/repo/docs")),
            Some(ProblemSeverity::Hint)
        );
        assert_eq!(
            store.worst_at(&place("/repo")),
            Some(ProblemSeverity::Error)
        );
        assert_eq!(store.worst_at(&place("/repo/clean.rs")), None);
        assert!(store.pushed(&place("/repo/src/b.rs"), Vec::new()));
        assert_eq!(
            store.worst_at(&place("/repo/src")),
            Some(ProblemSeverity::Warning)
        );
        store.forget(&place("/repo/src/a.rs"));
        assert_eq!(store.worst_at(&place("/repo/src")), None);
        assert_eq!(store.worst_at(&place("/repo")), Some(ProblemSeverity::Hint));
    }

    const BENCH_FILES: usize = 500;
    const BENCH_PER_FILE: u32 = 10;
    const BENCH_PASSES: u32 = 20;

    fn filled_store() -> DiagnosticStore {
        let mut store = DiagnosticStore::default();
        for file in 0..BENCH_FILES {
            let document = Document::new(
                &krad_domain::Host::Local,
                std::path::Path::new(&format!("/repo/src/module_{file}/file_{file}.rs")),
            );
            let found: Vec<lsp_types::Diagnostic> = (0..BENCH_PER_FILE)
                .map(|line| at(BENCH_PER_FILE - line, "mismatched types in this expression"))
                .collect();
            assert!(store.pushed(&document, found.clone()));
            assert!(!store.pulled(&document, found));
        }
        store
    }

    #[test]
    #[ignore = "benchmark, run with --ignored"]
    fn bench_problems_render_pass() -> std::io::Result<()> {
        use std::io::Write;
        let started = std::time::Instant::now();
        let mut store = filled_store();
        let filled = started.elapsed();
        let shown = |store: &DiagnosticStore| {
            store
                .rows()
                .of(&krad_domain::Host::Local)
                .map_or(0, |problems| problems.rows.len())
        };
        let mut seen = 0;
        let started = std::time::Instant::now();
        for _ in 0..BENCH_PASSES {
            store.rows.take();
            seen += shown(&store);
        }
        let rebuilt = started.elapsed() / BENCH_PASSES;
        let started = std::time::Instant::now();
        for _ in 0..BENCH_PASSES {
            seen += shown(&store);
        }
        let cached = started.elapsed() / BENCH_PASSES;
        writeln!(
            std::io::stderr(),
            "fill {filled:?}, render pass after a change {rebuilt:?}, render pass unchanged {cached:?}, rows {seen}"
        )
    }
}
