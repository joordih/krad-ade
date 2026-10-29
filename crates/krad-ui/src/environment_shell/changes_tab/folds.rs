use std::collections::HashSet;

use super::super::review_view::{DiffLine, LineKind};

const FOLD_CONTEXT: usize = 3;
const MIN_FOLDED: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiffRow {
    Line(usize),
    Fold { first: usize, hidden: usize },
}

pub(super) fn diff_rows(
    lines: &[DiffLine],
    unfolded: &HashSet<usize>,
    unfold_all: bool,
) -> Vec<DiffRow> {
    let shown: Vec<usize> = (0..lines.len())
        .filter(|index| lines[*index].kind != LineKind::Hunk)
        .collect();
    let is_context = |position: usize| lines[shown[position]].kind == LineKind::Context;
    let mut rows = Vec::with_capacity(shown.len());
    let mut position = 0;
    while position < shown.len() {
        if !is_context(position) {
            rows.push(DiffRow::Line(shown[position]));
            position += 1;
            continue;
        }
        let end = (position..shown.len())
            .find(|next| !is_context(*next))
            .unwrap_or(shown.len());
        let head = if position == 0 { 0 } else { FOLD_CONTEXT };
        let tail = if end == shown.len() { 0 } else { FOLD_CONTEXT };
        let run = end - position;
        let hidden = run.saturating_sub(head + tail);
        let first = shown.get(position + head).copied();
        match first.filter(|first| hidden >= MIN_FOLDED && !unfold_all && !unfolded.contains(first))
        {
            Some(first) => {
                rows.extend(
                    shown[position..position + head]
                        .iter()
                        .map(|i| DiffRow::Line(*i)),
                );
                rows.push(DiffRow::Fold { first, hidden });
                rows.extend(shown[end - tail..end].iter().map(|i| DiffRow::Line(*i)));
            }
            None => rows.extend(shown[position..end].iter().map(|i| DiffRow::Line(*i))),
        }
        position = end;
    }
    rows
}

pub(super) fn widest_row(rows: &[DiffRow], lines: &[DiffLine]) -> usize {
    rows.iter()
        .enumerate()
        .max_by_key(|(_, row)| match row {
            DiffRow::Line(index) => lines
                .get(*index)
                .map_or(0, |line| line.text.chars().count()),
            DiffRow::Fold { .. } => 0,
        })
        .map_or(0, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_shell::review_view::numbered_lines;

    fn whole_file(before: usize, after: usize) -> Vec<DiffLine> {
        let diff: Vec<String> = std::iter::once("@@ -1,9 +1,9 @@".to_owned())
            .chain((0..before).map(|line| format!(" a{line}")))
            .chain(["-old".to_owned(), "+new".to_owned()])
            .chain((0..after).map(|line| format!(" b{line}")))
            .collect();
        numbered_lines(&diff.join("\n"))
    }

    #[test]
    fn unchanged_runs_fold_around_the_change_keeping_three_lines_of_context() {
        let lines = whole_file(10, 10);
        let rows = diff_rows(&lines, &HashSet::new(), false);
        assert_eq!(
            rows[0],
            DiffRow::Fold {
                first: 1,
                hidden: 7
            }
        );
        assert_eq!(rows.len(), 1 + 3 + 2 + 3 + 1);
        assert_eq!(
            rows.last(),
            Some(&DiffRow::Fold {
                first: 16,
                hidden: 7
            })
        );
        assert!(
            !rows.contains(&DiffRow::Line(0)),
            "the hunk header does not show"
        );
    }

    #[test]
    fn an_unfolded_run_and_unfold_all_show_every_line() {
        let lines = whole_file(10, 10);
        let unfolded: HashSet<usize> = [1].into();
        let rows = diff_rows(&lines, &unfolded, false);
        assert_eq!(rows[0], DiffRow::Line(1));
        assert_eq!(
            rows.last(),
            Some(&DiffRow::Fold {
                first: 16,
                hidden: 7
            })
        );
        let all = diff_rows(&lines, &HashSet::new(), true);
        assert_eq!(all.len(), lines.len() - 1);
    }

    #[test]
    fn short_runs_never_fold() {
        let lines = whole_file(2, 4);
        let rows = diff_rows(&lines, &HashSet::new(), false);
        assert!(rows.iter().all(|row| matches!(row, DiffRow::Line(_))));
    }

    #[test]
    fn the_widest_row_holds_the_longest_line() {
        let lines = numbered_lines("@@ -1,3 +1,3 @@\n short\n-a much longer line\n+x\n");
        let rows = diff_rows(&lines, &HashSet::new(), true);
        assert_eq!(rows[widest_row(&rows, &lines)], DiffRow::Line(2));
    }
}
