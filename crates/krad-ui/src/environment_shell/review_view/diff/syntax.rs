use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};
use gpui_kit::component::input::Rope;
use gpui_kit::HighlightStyle;
use std::ops::Range;

use super::{DiffLine, LineKind};

const LINE_BREAK: char = '\n';

pub(in crate::environment_shell) type LineHighlights = Vec<(Range<usize>, HighlightStyle)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffSide {
    Old,
    New,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinePlace {
    side: DiffSide,
    range: Range<usize>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SideTexts {
    old: String,
    new: String,
    places: Vec<Option<LinePlace>>,
}

fn push_line(text: &mut String, line: &str) -> Range<usize> {
    if !text.is_empty() {
        text.push(LINE_BREAK);
    }
    let start = text.len();
    text.push_str(line);
    start..text.len()
}

fn side_texts(lines: &[DiffLine]) -> SideTexts {
    let mut texts = SideTexts::default();
    for line in lines {
        let place = match line.kind {
            LineKind::Hunk => None,
            LineKind::Removed => Some(LinePlace {
                side: DiffSide::Old,
                range: push_line(&mut texts.old, &line.text),
            }),
            LineKind::Added => Some(LinePlace {
                side: DiffSide::New,
                range: push_line(&mut texts.new, &line.text),
            }),
            LineKind::Context => {
                push_line(&mut texts.old, &line.text);
                Some(LinePlace {
                    side: DiffSide::New,
                    range: push_line(&mut texts.new, &line.text),
                })
            }
        };
        texts.places.push(place);
    }
    texts
}

fn line_highlights(
    styles: &[(Range<usize>, HighlightStyle)],
    line: &Range<usize>,
) -> LineHighlights {
    let first = styles.partition_point(|(range, _)| range.end <= line.start);
    styles
        .get(first..)
        .unwrap_or_default()
        .iter()
        .take_while(|(range, _)| range.start < line.end)
        .filter(|(_, style)| *style != HighlightStyle::default())
        .map(|(range, style)| {
            let start = range.start.max(line.start) - line.start;
            let end = range.end.min(line.end) - line.start;
            (start..end, *style)
        })
        .filter(|(range, _)| !range.is_empty())
        .collect()
}

fn side_styles(
    grammar: &str,
    text: &str,
    theme: &HighlightTheme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let rope = Rope::from(text);
    let mut highlighter = SyntaxHighlighter::new(grammar);
    highlighter.update(None, &rope, None);
    highlighter.styles(&(0..text.len()), theme)
}

#[must_use]
pub(in crate::environment_shell) fn highlight_diff(
    lines: &[DiffLine],
    grammar: &str,
    theme: &HighlightTheme,
) -> Vec<LineHighlights> {
    let texts = side_texts(lines);
    let old = side_styles(grammar, &texts.old, theme);
    let new = side_styles(grammar, &texts.new, theme);
    texts
        .places
        .iter()
        .map(|place| {
            place.as_ref().map_or_else(Vec::new, |place| {
                let styles = match place.side {
                    DiffSide::Old => &old,
                    DiffSide::New => &new,
                };
                line_highlights(styles, &place.range)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::numbered_lines;
    use super::*;
    use gpui_kit::{hsla, Hsla};

    const DIFF: &str = "@@ -1,3 +1,3 @@\n \
                        fn kept() {}\n\
                        -let old = 1;\n\
                        +let new = \"two\";\n \
                        // end\n";

    fn colored(color: Hsla) -> HighlightStyle {
        HighlightStyle {
            color: Some(color),
            ..HighlightStyle::default()
        }
    }

    #[test]
    fn removed_lines_read_the_old_side_and_the_rest_the_new_side() {
        let texts = side_texts(&numbered_lines(DIFF));
        assert_eq!(texts.old, "fn kept() {}\nlet old = 1;\n// end");
        assert_eq!(texts.new, "fn kept() {}\nlet new = \"two\";\n// end");
        assert_eq!(
            texts.places,
            [
                None,
                Some(LinePlace {
                    side: DiffSide::New,
                    range: 0..12,
                }),
                Some(LinePlace {
                    side: DiffSide::Old,
                    range: 13..25,
                }),
                Some(LinePlace {
                    side: DiffSide::New,
                    range: 13..29,
                }),
                Some(LinePlace {
                    side: DiffSide::New,
                    range: 30..36,
                }),
            ]
        );
    }

    #[test]
    fn a_span_across_lines_is_cut_to_each_line_and_counted_from_its_start() {
        let red = colored(hsla(0.0, 1.0, 0.5, 1.0));
        let blue = colored(hsla(0.6, 1.0, 0.5, 1.0));
        let styles = vec![
            (0..4, red),
            (4..8, HighlightStyle::default()),
            (8..14, blue),
            (14..20, red),
        ];
        assert_eq!(
            line_highlights(&styles, &(0..10)),
            [(0..4, red), (8..10, blue)]
        );
        assert_eq!(
            line_highlights(&styles, &(11..18)),
            [(0..3, blue), (3..7, red)]
        );
        assert_eq!(line_highlights(&styles, &(20..25)), []);
    }

    #[test]
    fn diff_lines_take_the_grammar_colors_of_their_side() {
        let theme = HighlightTheme::default_dark();
        let lines = numbered_lines(DIFF);
        let lit = highlight_diff(&lines, "rust", &theme);
        let style = |name: &str| theme.style.syntax.style(name);
        let at = |line: usize, range: Range<usize>| {
            lit[line]
                .iter()
                .find(|(span, _)| *span == range)
                .map(|(_, style)| *style)
        };
        assert_eq!(lit.len(), lines.len());
        assert_eq!(lit[0], LineHighlights::new(), "hunk headers stay plain");
        assert_eq!(at(1, 0..2), style("keyword"));
        assert_eq!(at(2, 0..3), style("keyword"));
        assert_eq!(at(3, 10..15), style("string"));
        assert_eq!(at(4, 0..6), style("comment"));
        assert!(highlight_diff(&lines, "text", &theme)
            .iter()
            .all(Vec::is_empty));
    }
}
