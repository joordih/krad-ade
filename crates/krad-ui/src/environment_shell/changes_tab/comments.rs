use krad_domain::{
    AnchorState, DiffAnchor, DiffComment, DiffCommentId, DiffQuote, DiffSide, HunkFingerprint,
    HunkLines, LineSpan,
};
use std::ops::{Range, RangeInclusive};
use std::path::Path;

use super::super::review_view::{DiffLine, LineKind};
use super::folds::DiffRow;

const ADDED_PREFIX: char = '+';
const REMOVED_PREFIX: char = '-';
const CONTEXT_PREFIX: char = ' ';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CommentPart {
    Header,
    Text(usize),
}

struct Hunk {
    body: Range<usize>,
    fingerprint: HunkFingerprint,
}

fn unified(line: &DiffLine) -> String {
    let prefix = match line.kind {
        LineKind::Added => ADDED_PREFIX,
        LineKind::Removed => REMOVED_PREFIX,
        LineKind::Context | LineKind::Hunk => CONTEXT_PREFIX,
    };
    format!("{prefix}{}", line.text)
}

fn hunks(lines: &[DiffLine]) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut bases: Vec<HunkFingerprint> = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        if lines[start].kind == LineKind::Hunk {
            start += 1;
        }
        let end = (start..lines.len())
            .find(|index| lines[*index].kind == LineKind::Hunk)
            .unwrap_or(lines.len());
        if end > start {
            let texts: Vec<String> = lines[start..end].iter().map(unified).collect();
            let base: HunkFingerprint = texts.iter().map(String::as_str).collect();
            let earlier = bases.iter().filter(|seen| **seen == base).count();
            bases.push(base);
            hunks.push(Hunk {
                body: start..end,
                fingerprint: base.occurrence(earlier),
            });
        }
        start = end;
    }
    hunks
}

fn span_of(lines: &[DiffLine]) -> Option<LineSpan> {
    let side_span = |side, numbers: Vec<u32>| {
        Some(LineSpan {
            side,
            first: *numbers.first()?,
            last: *numbers.last()?,
        })
    };
    let new: Vec<u32> = lines.iter().filter_map(|line| line.new).collect();
    side_span(DiffSide::New, new).or_else(|| {
        side_span(
            DiffSide::Old,
            lines.iter().filter_map(|line| line.old).collect(),
        )
    })
}

pub(super) fn quote(
    lines: &[DiffLine],
    path: &Path,
    one: usize,
    other: usize,
) -> Option<DiffQuote> {
    let picked = HunkLines::new(one, other);
    let hunk = hunks(lines)
        .into_iter()
        .find(|hunk| hunk.body.contains(&one) && hunk.body.contains(&other))?;
    let quoted = lines.get(picked.range())?;
    let start = hunk.body.start;
    Some(DiffQuote {
        anchor: DiffAnchor {
            path: path.to_path_buf(),
            hunk: hunk.fingerprint,
            lines: HunkLines::new(one - start, other - start),
        },
        span: span_of(quoted)?,
        excerpt: quoted.iter().map(unified).collect(),
    })
}

pub(super) fn same_hunk(lines: &[DiffLine], one: usize, other: usize) -> bool {
    hunks(lines)
        .iter()
        .any(|hunk| hunk.body.contains(&one) && hunk.body.contains(&other))
}

pub(super) fn locate(lines: &[DiffLine], anchor: &DiffAnchor) -> Option<RangeInclusive<usize>> {
    let hunk = hunks(lines)
        .into_iter()
        .find(|hunk| hunk.fingerprint == anchor.hunk)?;
    let offsets = anchor.lines.range();
    let located = hunk.body.start + offsets.start()..=hunk.body.start + offsets.end();
    hunk.body.contains(located.end()).then_some(located)
}

pub(super) fn relocate(lines: &[DiffLine], anchor: &DiffAnchor) -> Option<(usize, usize)> {
    locate(lines, anchor).map(|range| (*range.start(), *range.end()))
}

pub(super) fn reanchor(lines: &[DiffLine], comments: &mut [DiffComment]) -> bool {
    let mut changed = false;
    for comment in comments {
        let located = locate(lines, &comment.quote.anchor)
            .and_then(|range| lines.get(range))
            .and_then(span_of);
        let (state, span) = match located {
            Some(span) => (AnchorState::Current, span),
            None => (AnchorState::Stale, comment.quote.span),
        };
        changed |= comment.state != state || comment.quote.span != span;
        comment.state = state;
        comment.quote.span = span;
    }
    changed
}

pub(super) struct PlacedComment {
    pub(super) id: DiffCommentId,
    pub(super) after: Option<usize>,
    pub(super) text_rows: usize,
}

impl PlacedComment {
    pub(super) fn new(lines: &[DiffLine], comment: &DiffComment) -> Self {
        Self {
            id: comment.id,
            after: locate(lines, &comment.quote.anchor).map(|range| *range.end()),
            text_rows: comment.text.lines().count().max(1),
        }
    }

    fn rows(&self) -> impl Iterator<Item = DiffRow> + '_ {
        std::iter::once(CommentPart::Header)
            .chain((0..self.text_rows).map(CommentPart::Text))
            .map(|part| DiffRow::Comment { id: self.id, part })
    }
}

const fn row_start(row: DiffRow) -> Option<usize> {
    match row {
        DiffRow::Line(index) | DiffRow::Fold { first: index, .. } => Some(index),
        DiffRow::Comment { .. } => None,
    }
}

pub(super) fn with_comments(rows: Vec<DiffRow>, placed: &[PlacedComment]) -> Vec<DiffRow> {
    let mut out: Vec<DiffRow> = placed
        .iter()
        .filter(|comment| comment.after.is_none())
        .flat_map(PlacedComment::rows)
        .collect();
    let mut anchored: Vec<&PlacedComment> = placed
        .iter()
        .filter(|comment| comment.after.is_some())
        .collect();
    anchored.sort_by_key(|comment| comment.after);
    let mut pending = anchored.into_iter().peekable();
    let starts: Vec<Option<usize>> = rows.iter().copied().map(row_start).collect();
    for (position, row) in rows.into_iter().enumerate() {
        out.push(row);
        let next_start = starts[position + 1..].iter().flatten().next().copied();
        while let Some(comment) =
            pending.next_if(|comment| next_start.is_none_or(|next| comment.after < Some(next)))
        {
            out.extend(comment.rows());
        }
    }
    out.extend(pending.flat_map(PlacedComment::rows));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_shell::review_view::numbered_lines;
    use std::path::PathBuf;

    const FILE: &str = "/w/api/src/lib.rs";
    const DIFF: &str = "@@ -10,3 +10,3 @@ fn a()\n keep();\n-old();\n+new();\n tail();\n\
                        @@ -40,2 +40,3 @@\n x();\n+y();\n z();\n";
    const SHIFTED: &str = "@@ -10,3 +14,3 @@ fn a()\n keep();\n-old();\n+new();\n tail();\n\
                           @@ -40,2 +44,3 @@\n x();\n+y();\n z();\n";
    const EDITED: &str = "@@ -10,3 +10,3 @@ fn a()\n keep();\n-old();\n+newer();\n tail();\n\
                          @@ -40,2 +40,3 @@\n x();\n+y();\n z();\n";

    fn comment(quote: DiffQuote) -> DiffComment {
        DiffComment {
            id: DiffCommentId(1),
            quote,
            text: "rename it".to_owned(),
            state: AnchorState::Current,
        }
    }

    #[test]
    fn a_quote_keeps_the_commented_lines_and_their_numbers() {
        let lines = numbered_lines(DIFF);
        let Some(quote) = quote(&lines, Path::new(FILE), 3, 2) else {
            unreachable!("both lines are in the first hunk");
        };
        assert_eq!(quote.anchor.path, PathBuf::from(FILE));
        assert_eq!(quote.anchor.lines.range(), 1..=2);
        assert_eq!(quote.excerpt, ["-old();", "+new();"]);
        assert_eq!(
            quote.span,
            LineSpan {
                side: DiffSide::New,
                first: 11,
                last: 11
            }
        );
        let removed = super::quote(&lines, Path::new(FILE), 2, 2).map(|quote| quote.span);
        assert_eq!(
            removed,
            Some(LineSpan {
                side: DiffSide::Old,
                first: 11,
                last: 11
            })
        );
    }

    #[test]
    fn a_draft_follows_its_lines_when_the_diff_is_read_again() {
        let lines = numbered_lines(DIFF);
        let anchor = quote(&lines, Path::new(FILE), 7, 7).map(|quoted| quoted.anchor);
        let Some(anchor) = anchor else {
            unreachable!("line 7 is in the second hunk");
        };
        let grown = format!("@@ -1,1 +1,2 @@\n a();\n+b();\n{DIFF}");
        let moved = numbered_lines(&grown);
        assert_eq!(relocate(&moved, &anchor), Some((10, 10)));
        assert_eq!(moved[10].text, "y();");
        let edited = numbered_lines(EDITED);
        let first = quote(&lines, Path::new(FILE), 2, 3).map(|quoted| quoted.anchor);
        assert_eq!(first.and_then(|anchor| relocate(&edited, &anchor)), None);
    }

    #[test]
    fn two_identical_hunks_anchor_apart() {
        let twice = "@@ -1,1 +1,1 @@\n-a\n+b\n@@ -9,1 +9,1 @@\n-a\n+b\n";
        let lines = numbered_lines(twice);
        let second = quote(&lines, Path::new(FILE), 4, 5).map(|quoted| quoted.anchor);
        let Some(second) = second else {
            unreachable!("lines 4 and 5 are the second hunk");
        };
        assert_eq!(relocate(&lines, &second), Some((4, 5)));
    }

    #[test]
    fn a_range_across_hunks_or_on_a_header_is_not_quoted() {
        let lines = numbered_lines(DIFF);
        assert!(quote(&lines, Path::new(FILE), 3, 7).is_none());
        assert!(quote(&lines, Path::new(FILE), 0, 1).is_none());
        assert!(!same_hunk(&lines, 3, 7));
        assert!(same_hunk(&lines, 6, 8));
    }

    #[test]
    fn a_comment_follows_its_hunk_when_lines_shift_and_goes_stale_when_it_changes() {
        let lines = numbered_lines(DIFF);
        let Some(quote) = quote(&lines, Path::new(FILE), 7, 7) else {
            unreachable!("the added line is quotable");
        };
        let mut comments = [comment(quote)];

        let shifted = numbered_lines(SHIFTED);
        assert_eq!(locate(&shifted, &comments[0].quote.anchor), Some(7..=7));
        assert!(reanchor(&shifted, &mut comments));
        assert_eq!(comments[0].state, AnchorState::Current);
        assert_eq!(
            (comments[0].quote.span.first, comments[0].quote.span.last),
            (45, 45)
        );

        let first_hunk = numbered_lines(DIFF);
        let Some(on_first) = super::quote(&first_hunk, Path::new(FILE), 3, 3) else {
            unreachable!("the added line is quotable");
        };
        let mut edited = [comment(on_first)];
        assert!(reanchor(&numbered_lines(EDITED), &mut edited));
        assert_eq!(edited[0].state, AnchorState::Stale);
        assert_eq!(
            edited[0].quote.span.first, 11,
            "a stale comment keeps its last lines"
        );
        assert!(!reanchor(&numbered_lines(EDITED), &mut edited));
        assert!(reanchor(&first_hunk, &mut edited));
        assert_eq!(edited[0].state, AnchorState::Current);
    }

    #[test]
    fn comment_rows_follow_their_last_line_and_stale_ones_lead() {
        let rows = vec![
            DiffRow::Line(1),
            DiffRow::Line(2),
            DiffRow::Fold {
                first: 3,
                hidden: 4,
            },
            DiffRow::Line(9),
        ];
        let placed = [
            PlacedComment {
                id: DiffCommentId(1),
                after: Some(2),
                text_rows: 2,
            },
            PlacedComment {
                id: DiffCommentId(2),
                after: None,
                text_rows: 1,
            },
            PlacedComment {
                id: DiffCommentId(3),
                after: Some(5),
                text_rows: 1,
            },
        ];
        let header = |id| DiffRow::Comment {
            id: DiffCommentId(id),
            part: CommentPart::Header,
        };
        let text = |id, line| DiffRow::Comment {
            id: DiffCommentId(id),
            part: CommentPart::Text(line),
        };
        assert_eq!(
            with_comments(rows, &placed),
            [
                header(2),
                text(2, 0),
                DiffRow::Line(1),
                DiffRow::Line(2),
                header(1),
                text(1, 0),
                text(1, 1),
                DiffRow::Fold {
                    first: 3,
                    hidden: 4
                },
                header(3),
                text(3, 0),
                DiffRow::Line(9),
            ]
        );
    }
}
