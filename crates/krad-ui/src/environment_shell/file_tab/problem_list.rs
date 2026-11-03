use gpui_kit::component::input::{Rope, RopeExt as _};
use gpui_kit::SharedString;
use krad_domain::ProblemSeverity;
use krad_lsp::lsp_types;
use std::ops::Range;
use std::sync::Arc;

use super::super::language_servers::problem_severity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct EditorProblem {
    pub(in crate::environment_shell) range: Range<usize>,
    pub(in crate::environment_shell) line: usize,
    pub(in crate::environment_shell) severity: ProblemSeverity,
    pub(in crate::environment_shell) message: SharedString,
    pub(in crate::environment_shell) source: Option<SharedString>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::environment_shell) struct ProblemList {
    pub(in crate::environment_shell) problems: Arc<[EditorProblem]>,
}

impl ProblemList {
    pub(in crate::environment_shell) fn from_diagnostics(
        diagnostics: &[lsp_types::Diagnostic],
        text: &Rope,
    ) -> Self {
        let lines = text.lines_len();
        let mut problems: Vec<EditorProblem> = diagnostics
            .iter()
            .filter(|diagnostic| (diagnostic.range.end.line as usize) < lines)
            .map(|diagnostic| {
                let start = text.position_to_offset(&diagnostic.range.start);
                let end = text.position_to_offset(&diagnostic.range.end).max(start);
                EditorProblem {
                    range: start..end,
                    line: diagnostic.range.start.line as usize,
                    severity: problem_severity(diagnostic),
                    message: diagnostic.message.clone().into(),
                    source: diagnostic.source.clone().map(SharedString::from),
                }
            })
            .collect();
        problems.sort_by_key(|problem| (problem.range.start, problem.severity));
        Self {
            problems: problems.into(),
        }
    }

    pub(in crate::environment_shell) fn at(&self, offset: usize) -> Vec<EditorProblem> {
        self.problems
            .iter()
            .filter(|problem| problem.range.start <= offset && offset <= problem.range.end)
            .cloned()
            .collect()
    }

    pub(in crate::environment_shell) fn in_lines(
        &self,
        lines: Range<usize>,
    ) -> impl Iterator<Item = &EditorProblem> {
        self.problems
            .iter()
            .filter(move |problem| lines.contains(&problem.line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::{DiagnosticSeverity, Position};

    fn problem(
        line: u32,
        from: u32,
        to: u32,
        severity: DiagnosticSeverity,
    ) -> lsp_types::Diagnostic {
        lsp_types::Diagnostic {
            range: lsp_types::Range::new(Position::new(line, from), Position::new(line, to)),
            severity: Some(severity),
            message: format!("on {line}"),
            ..lsp_types::Diagnostic::default()
        }
    }

    #[test]
    fn problems_take_byte_ranges_and_drop_lines_the_text_no_longer_has() {
        let text = Rope::from("let a = 1;\nlet bé = 2;\n");
        let list = ProblemList::from_diagnostics(
            &[
                problem(1, 4, 6, DiagnosticSeverity::ERROR),
                problem(0, 4, 5, DiagnosticSeverity::WARNING),
                problem(9, 0, 1, DiagnosticSeverity::ERROR),
            ],
            &text,
        );
        assert_eq!(list.problems.len(), 2);
        assert_eq!(list.problems[0].range, 4..5);
        assert_eq!(list.problems[1].range, 15..18);
        assert_eq!(list.at(16).len(), 1);
        assert_eq!(list.at(16)[0].severity, ProblemSeverity::Error);
        assert_eq!(list.at(9), Vec::new());
        assert_eq!(list.in_lines(1..2).count(), 1);
    }
}
