use gpui_kit::SharedString;
use krad_domain::Host;
use krad_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position};
use std::collections::HashMap;
use std::path::PathBuf;

use super::super::Document;
use super::severity_of;

const LINE_BASE: u32 = 1;

#[derive(Debug, Default)]
pub(in crate::environment_shell) struct ProblemRows {
    hosts: Vec<HostProblems>,
}

#[derive(Debug)]
pub(in crate::environment_shell) struct HostProblems {
    host: Host,
    pub(in crate::environment_shell) rows: Vec<ProblemRow>,
    pub(in crate::environment_shell) errors: usize,
}

#[derive(Debug)]
pub(in crate::environment_shell) enum ProblemRow {
    File(FileRow),
    Problem(ProblemLine),
}

#[derive(Debug)]
pub(in crate::environment_shell) struct FileRow {
    pub(in crate::environment_shell) path: PathBuf,
    pub(in crate::environment_shell) name: SharedString,
    pub(in crate::environment_shell) errors: SharedString,
    pub(in crate::environment_shell) warnings: SharedString,
}

#[derive(Debug)]
pub(in crate::environment_shell) struct ProblemLine {
    pub(in crate::environment_shell) file: usize,
    pub(in crate::environment_shell) start: Position,
    pub(in crate::environment_shell) severity: DiagnosticSeverity,
    pub(in crate::environment_shell) message: SharedString,
    pub(in crate::environment_shell) place: SharedString,
}

fn count_of(problems: &[Diagnostic], severity: DiagnosticSeverity) -> usize {
    problems
        .iter()
        .filter(|problem| severity_of(problem) == severity)
        .count()
}

impl ProblemRows {
    pub(super) fn new(merged: &HashMap<Document, Vec<Diagnostic>>) -> Self {
        let mut files: Vec<(&Document, &Vec<Diagnostic>)> = merged
            .iter()
            .filter(|(_, problems)| !problems.is_empty())
            .collect();
        files.sort_by(|(left, _), (right, _)| left.path.cmp(&right.path));
        let mut hosts: Vec<HostProblems> = Vec::new();
        for (document, problems) in files {
            let index = hosts
                .iter()
                .position(|shown| shown.host == document.host)
                .unwrap_or_else(|| {
                    hosts.push(HostProblems::new(&document.host));
                    hosts.len() - 1
                });
            hosts[index].push(document, problems);
        }
        Self { hosts }
    }

    pub(in crate::environment_shell) fn of(&self, host: &Host) -> Option<&HostProblems> {
        self.hosts.iter().find(|shown| &shown.host == host)
    }
}

impl HostProblems {
    fn new(host: &Host) -> Self {
        Self {
            host: host.clone(),
            rows: Vec::new(),
            errors: 0,
        }
    }

    fn push(&mut self, document: &Document, problems: &[Diagnostic]) {
        let errors = count_of(problems, DiagnosticSeverity::ERROR);
        let warnings = count_of(problems, DiagnosticSeverity::WARNING);
        self.errors += errors;
        let file = self.rows.len();
        let name = document.path.file_name().map_or_else(
            || document.path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.rows.push(ProblemRow::File(FileRow {
            path: document.path.clone(),
            name: name.into(),
            errors: errors.to_string().into(),
            warnings: warnings.to_string().into(),
        }));
        self.rows.extend(
            problems
                .iter()
                .map(|problem| ProblemRow::Problem(line(file, problem))),
        );
    }
}

fn line(file: usize, problem: &Diagnostic) -> ProblemLine {
    let start = problem.range.start;
    ProblemLine {
        file,
        start,
        severity: severity_of(problem),
        message: problem
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
            .into(),
        place: format!("{}:{}", start.line + LINE_BASE, start.character + LINE_BASE).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::Range;
    use std::path::Path;

    fn with(severity: Option<DiagnosticSeverity>, line: u32) -> Diagnostic {
        Diagnostic {
            range: Range::new(Position::new(line, 2), Position::new(line, 3)),
            severity,
            message: format!("problem {line}\nmore detail"),
            ..Diagnostic::default()
        }
    }

    #[test]
    fn counts_split_errors_from_warnings() {
        let problems = [
            with(Some(DiagnosticSeverity::ERROR), 0),
            with(None, 1),
            with(Some(DiagnosticSeverity::WARNING), 2),
        ];
        assert_eq!(count_of(&problems, DiagnosticSeverity::ERROR), 2);
        assert_eq!(count_of(&problems, DiagnosticSeverity::WARNING), 1);
    }

    #[test]
    fn files_of_each_host_list_in_path_order_with_their_problems_under_them(
    ) -> Result<(), &'static str> {
        let local = |path: &str| Document::new(&Host::Local, Path::new(path));
        let merged = HashMap::from([
            (local("/repo/b.rs"), vec![with(None, 4)]),
            (
                local("/repo/a.rs"),
                vec![with(Some(DiagnosticSeverity::WARNING), 0), with(None, 9)],
            ),
            (local("/repo/clean.rs"), Vec::new()),
        ]);
        let rows = ProblemRows::new(&merged);
        let shown = rows
            .of(&Host::Local)
            .ok_or("no problems for this computer")?;
        assert_eq!(shown.errors, 2);
        let outline: Vec<String> = shown
            .rows
            .iter()
            .map(|row| match row {
                ProblemRow::File(file) => {
                    format!("{} {} {}", file.name, file.errors, file.warnings)
                }
                ProblemRow::Problem(problem) => {
                    format!("{} {} {}", problem.file, problem.message, problem.place)
                }
            })
            .collect();
        assert_eq!(
            outline,
            [
                "a.rs 1 1",
                "0 problem 0 1:3",
                "0 problem 9 10:3",
                "b.rs 1 0",
                "3 problem 4 5:3"
            ]
        );
        Ok(())
    }
}
