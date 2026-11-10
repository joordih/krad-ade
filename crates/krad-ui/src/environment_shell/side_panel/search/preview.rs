use krad_repo::{FoundLine, ProjectMatches};
use std::ops::Range;

const CONTEXT_BEFORE: usize = 24;
const MAX_SHOWN_BYTES: usize = 240;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchRow {
    File(usize),
    Line { file: usize, line: usize },
}

pub(super) fn search_rows(found: &ProjectMatches) -> Vec<SearchRow> {
    found
        .files
        .iter()
        .enumerate()
        .flat_map(|(file, matches)| {
            std::iter::once(SearchRow::File(file))
                .chain((0..matches.lines.len()).map(move |line| SearchRow::Line { file, line }))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    Found,
    Removed,
    Inserted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LinePreview {
    pub(super) text: String,
    pub(super) marks: Vec<(Range<usize>, Mark)>,
}

pub(super) fn line_preview(found: &FoundLine, replacement: Option<&str>) -> LinePreview {
    let start = shown_start(found);
    let mut preview = LinePreview {
        text: String::new(),
        marks: Vec::new(),
    };
    let mut from = start;
    for range in found.matches.iter().filter(|range| range.start >= start) {
        let (Some(kept), Some(matched)) = (
            found.text.get(from..range.start),
            found.text.get(range.clone()),
        ) else {
            continue;
        };
        if preview.text.len() + kept.len() >= MAX_SHOWN_BYTES {
            break;
        }
        preview.text.push_str(kept);
        let mark = if replacement.is_some() {
            Mark::Removed
        } else {
            Mark::Found
        };
        preview.push(matched, mark);
        if let Some(replacement) = replacement {
            preview.push(replacement, Mark::Inserted);
        }
        from = range.end;
    }
    let rest = found.text.get(from..).unwrap_or_default().trim_end();
    preview.text.push_str(cut(
        rest,
        MAX_SHOWN_BYTES.saturating_sub(preview.text.len()),
    ));
    preview
}

impl LinePreview {
    fn push(&mut self, text: &str, mark: Mark) {
        let at = self.text.len();
        self.text.push_str(text);
        if !text.is_empty() {
            self.marks.push((at..self.text.len(), mark));
        }
    }
}

fn shown_start(found: &FoundLine) -> usize {
    let indent = found.text.len() - found.text.trim_start().len();
    let first = found.matches.first().map_or(indent, |range| range.start);
    let wanted = first.saturating_sub(CONTEXT_BEFORE).max(indent).min(first);
    (wanted..=first)
        .find(|at| found.text.is_char_boundary(*at))
        .unwrap_or(first)
}

fn cut(text: &str, max: usize) -> &str {
    let end = (0..=max.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or_default();
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::FileMatches;
    use std::path::PathBuf;

    fn found(text: &str, spans: &[(usize, usize)]) -> FoundLine {
        FoundLine {
            line: 1,
            text: text.to_owned(),
            matches: spans.iter().map(|&(start, end)| start..end).collect(),
        }
    }

    #[test]
    fn each_file_heads_its_matching_lines() {
        let file = |name: &str, lines: usize| FileMatches {
            path: PathBuf::from(name),
            lines: (0..lines).map(|_| found("x", &[(0, 1)])).collect(),
        };
        let matches = ProjectMatches {
            files: vec![file("a", 2), file("b", 1)],
            truncated: false,
        };
        assert_eq!(
            search_rows(&matches),
            [
                SearchRow::File(0),
                SearchRow::Line { file: 0, line: 0 },
                SearchRow::Line { file: 0, line: 1 },
                SearchRow::File(1),
                SearchRow::Line { file: 1, line: 0 },
            ]
        );
        assert_eq!(search_rows(&ProjectMatches::default()), []);
    }

    #[test]
    fn the_preview_marks_matches_and_drops_the_indent() {
        let preview = line_preview(&found("    let foo = foo;", &[(8, 11), (14, 17)]), None);
        assert_eq!(preview.text, "let foo = foo;");
        assert_eq!(preview.marks, [(4..7, Mark::Found), (10..13, Mark::Found)]);
    }

    #[test]
    fn a_replacement_shows_the_old_text_beside_the_new() {
        let preview = line_preview(&found("foo bar", &[(0, 3)]), Some("baz"));
        assert_eq!(preview.text, "foobaz bar");
        assert_eq!(
            preview.marks,
            [(0..3, Mark::Removed), (3..6, Mark::Inserted)]
        );
        let removal = line_preview(&found("foo bar", &[(0, 3)]), Some(""));
        assert_eq!(removal.marks, [(0..3, Mark::Removed)]);
    }

    #[test]
    fn a_long_line_is_cut_however_far_its_next_match_is() {
        let text = format!("foo{}foo", "x".repeat(4 * MAX_SHOWN_BYTES));
        let far = text.len() - "foo".len();
        let preview = line_preview(&found(&text, &[(0, 3), (far, text.len())]), None);
        assert!(preview.text.len() <= MAX_SHOWN_BYTES);
        assert_eq!(preview.marks, [(0..3, Mark::Found)]);
    }

    #[test]
    fn a_far_match_keeps_some_context_before_it() {
        let text = format!("{}needle", "é".repeat(40));
        let start = text.len() - "needle".len();
        let preview = line_preview(&found(&text, &[(start, text.len())]), None);
        assert!(preview.text.ends_with("needle"));
        assert!(preview.text.len() <= CONTEXT_BEFORE + "needle".len());
        assert_eq!(preview.marks.len(), 1);
    }
}
