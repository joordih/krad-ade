use std::ops::Range;

use super::query::HistoryQuery;
use super::tokens::tokens;

const SNIPPET_CHARS: usize = 96;
const LEAD_CHARS: usize = 24;
const ELLIPSIS: &str = "\u{2026}";
const SPACE: char = ' ';

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snippet {
    pub text: String,
    pub marks: Vec<Range<usize>>,
}

fn matched_spans(text: &str, query: &HistoryQuery) -> Vec<Range<usize>> {
    tokens(text)
        .filter(|token| (0..query.terms.len()).any(|index| query.matches(index, &token.term)))
        .map(|token| token.span)
        .collect()
}

fn window(text: &str, anchor: usize) -> Range<usize> {
    let starts: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    let count = starts.len();
    let anchor_char = starts.partition_point(|start| *start < anchor);
    let end_char = (anchor_char.saturating_sub(LEAD_CHARS) + SNIPPET_CHARS).min(count);
    let start_char = end_char.saturating_sub(SNIPPET_CHARS);
    let byte = |char_index: usize| starts.get(char_index).copied().unwrap_or(text.len());
    byte(start_char)..byte(end_char)
}

#[must_use]
pub fn snippet(text: &str, query: &HistoryQuery) -> Snippet {
    let spans = matched_spans(text, query);
    let shown = window(text, spans.first().map_or(0, |span| span.start));
    let mut out = Snippet::default();
    if shown.start > 0 {
        out.text.push_str(ELLIPSIS);
    }
    let mut open: Option<usize> = None;
    let mut pending_space = false;
    for (offset, character) in text[shown.clone()].char_indices() {
        let at = shown.start + offset;
        if character.is_whitespace() {
            pending_space = !out.text.is_empty() && !out.text.ends_with(ELLIPSIS);
            continue;
        }
        if pending_space {
            out.text.push(SPACE);
            pending_space = false;
        }
        if spans.iter().any(|span| span.start == at) {
            open = Some(out.text.len());
        }
        out.text.push(character);
        let after = at + character.len_utf8();
        if let Some(start) = open.filter(|_| spans.iter().any(|span| span.end == after)) {
            out.marks.push(start..out.text.len());
            open = None;
        }
    }
    if let Some(start) = open {
        out.marks.push(start..out.text.len());
    }
    if shown.end < text.len() {
        out.text.push_str(ELLIPSIS);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(text: &str, query: &str) -> Snippet {
        HistoryQuery::parse(query).map_or_else(Snippet::default, |query| snippet(text, &query))
    }

    fn marked(found: &Snippet) -> Vec<&str> {
        found
            .marks
            .iter()
            .map(|mark| &found.text[mark.clone()])
            .collect()
    }

    #[test]
    fn a_short_message_shows_whole_with_its_matches_marked() {
        let found = cut("Fix the Race in the reader", "race read");
        assert_eq!(found.text, "Fix the Race in the reader");
        assert_eq!(marked(&found), ["Race", "reader"]);
    }

    #[test]
    fn line_breaks_and_runs_of_spaces_become_one_space() {
        let found = cut("  First line\n\n   second\tline  ", "second");
        assert_eq!(found.text, "First line second line");
        assert_eq!(marked(&found), ["second"]);
    }

    #[test]
    fn a_long_message_is_cut_around_the_first_match_with_ellipses() {
        let before = "word ".repeat(40);
        let after = " tail".repeat(40);
        let text = format!("{before}needle{after}");
        let found = cut(&text, "needle ");
        assert!(found.text.starts_with(ELLIPSIS));
        assert!(found.text.ends_with(ELLIPSIS));
        assert_eq!(marked(&found), ["needle"]);
        let shown = found.text.chars().count();
        assert!(shown <= SNIPPET_CHARS + 2, "{shown} characters shown");
        let lead = found.text.find("needle").unwrap_or(found.text.len());
        assert!(found.text[..lead].chars().count() <= LEAD_CHARS + 1);
    }

    #[test]
    fn a_match_near_the_end_keeps_a_full_window_before_it() {
        let text = format!("{}needle", "a ".repeat(100));
        let found = cut(&text, "needle ");
        assert!(found.text.starts_with(ELLIPSIS));
        assert!(found.text.ends_with("needle"));
        assert!(found.text.chars().count() > SNIPPET_CHARS / 2);
    }

    #[test]
    fn marks_land_on_character_boundaries_of_multibyte_text() {
        let found = cut("Ça marche déjà très bien", "déj");
        assert_eq!(marked(&found), ["déjà"]);
    }

    #[test]
    fn a_message_without_matches_shows_its_start() {
        let found = cut(&"x".repeat(200), "zz");
        assert_eq!(found.marks, []);
        assert!(!found.text.starts_with(ELLIPSIS));
        assert!(found.text.ends_with(ELLIPSIS));
    }
}
