use gpui_kit::component::input::{
    EditorState, GutterDecoration, GutterDecorationCollection, OverviewRulerDecoration,
    OverviewRulerDecorationCollection, OverviewRulerLane, Rope, RopeExt as _,
};
use gpui_kit::{rgb, App, Entity, Hsla};
use krad_domain::{ChangeKind, ProblemSeverity};
use std::ops::Range;

use super::super::file_tree::change_color;
use super::problem_list::ProblemList;
use crate::theme::palette;

const PROBLEM_LANE: OverviewRulerLane = OverviewRulerLane::Right;
const CHANGE_LANE: OverviewRulerLane = OverviewRulerLane::Left;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LineChange {
    pub(super) lines: Range<usize>,
    pub(super) kind: ChangeKind,
}

#[derive(Clone, Debug, Default)]
pub(in crate::environment_shell) struct EditorMarks {
    problems: OverviewRulerDecorationCollection,
    changes: OverviewRulerDecorationCollection,
    change_bars: GutterDecorationCollection,
}

impl EditorMarks {
    pub(super) fn new(state: &Entity<EditorState>, cx: &mut App) -> Self {
        state.update(cx, |state, cx| Self {
            problems: state.create_overview_ruler_decorations_collection(Vec::new(), cx),
            changes: state.create_overview_ruler_decorations_collection(Vec::new(), cx),
            change_bars: state.create_gutter_decorations_collection(Vec::new(), cx),
        })
    }

    pub(in crate::environment_shell) fn show_problems(
        &self,
        problems: &ProblemList,
        state: &Entity<EditorState>,
        cx: &mut App,
    ) {
        let color = rgb(palette(cx).error_fg).into();
        let marks = problem_marks(problems, state.read(cx).text(), color);
        self.problems.set(marks, cx);
    }

    pub(super) fn show_changes(
        &self,
        changes: &[LineChange],
        state: &Entity<EditorState>,
        cx: &mut App,
    ) {
        let pal = palette(cx);
        let colored: Vec<(Range<usize>, Hsla)> = {
            let text = state.read(cx).text();
            changes
                .iter()
                .map(|change| {
                    (
                        line_bytes(text, &change.lines),
                        rgb(change_color(change.kind, pal)).into(),
                    )
                })
                .collect()
        };
        let ruler = colored
            .iter()
            .map(|(range, color)| {
                OverviewRulerDecoration::new(range.clone())
                    .with_lane(CHANGE_LANE)
                    .with_color(*color)
            })
            .collect();
        let bars = colored
            .into_iter()
            .map(|(range, color)| GutterDecoration::new(range).with_color(color))
            .collect();
        self.changes.set(ruler, cx);
        self.change_bars.set(bars, cx);
    }
}

fn line_bytes(text: &Rope, lines: &Range<usize>) -> Range<usize> {
    let offset = |line: usize| {
        if line < text.lines_len() {
            text.line_start_offset(line)
        } else {
            text.len()
        }
    };
    offset(lines.start)..offset(lines.end)
}

fn problem_marks(problems: &ProblemList, text: &Rope, color: Hsla) -> Vec<OverviewRulerDecoration> {
    let mut lines: Vec<usize> = problems
        .problems
        .iter()
        .filter(|problem| problem.severity == ProblemSeverity::Error)
        .map(|problem| problem.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
        .into_iter()
        .map(|line| {
            OverviewRulerDecoration::new(line_bytes(text, &(line..line + 1)))
                .with_lane(PROBLEM_LANE)
                .with_color(color)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::problem_list::EditorProblem;
    use super::*;
    use gpui_kit::SharedString;

    fn problem(line: usize, severity: ProblemSeverity) -> EditorProblem {
        EditorProblem {
            range: 0..0,
            line,
            severity,
            message: SharedString::default(),
            source: None,
        }
    }

    #[test]
    fn a_line_range_covers_whole_lines_and_ends_at_the_document_end() {
        let text = Rope::from("one\ntwo\nthree");
        assert_eq!(line_bytes(&text, &(1..2)), 4..8);
        assert_eq!(line_bytes(&text, &(1..3)), 4..13);
        assert_eq!(line_bytes(&text, &(2..2)), 8..8);
        assert_eq!(line_bytes(&text, &(3..3)), 13..13);
    }

    #[test]
    fn only_errors_mark_the_diagnostics_lane_once_per_line() {
        let text = Rope::from("one\ntwo\nthree");
        let problems = ProblemList {
            problems: [
                problem(2, ProblemSeverity::Error),
                problem(1, ProblemSeverity::Warning),
                problem(2, ProblemSeverity::Error),
                problem(0, ProblemSeverity::Error),
            ]
            .into(),
        };
        let marks = problem_marks(&problems, &text, Hsla::default());
        let ranges: Vec<Range<usize>> = marks.iter().map(|mark| mark.range().clone()).collect();
        assert_eq!(ranges, [0..4, 8..13]);
        assert!(marks.iter().all(|mark| mark.lane() == PROBLEM_LANE));
    }
}
