use gpui_kit::SharedString;
use std::ops::Range;

const LINE_BREAK: &str = "\n";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) struct TextPoint {
    pub(super) line: usize,
    pub(super) byte: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct CodeSelection {
    anchor: TextPoint,
    head: TextPoint,
}

impl CodeSelection {
    pub(super) const fn at(point: TextPoint) -> Self {
        Self {
            anchor: point,
            head: point,
        }
    }

    #[must_use]
    pub(super) const fn extended_to(self, head: TextPoint) -> Self {
        Self {
            anchor: self.anchor,
            head,
        }
    }

    fn ordered(self) -> (TextPoint, TextPoint) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub(super) fn range_on(self, line: usize, len: usize) -> Option<Range<usize>> {
        let (start, end) = self.ordered();
        if line < start.line || line > end.line {
            return None;
        }
        let from = if line == start.line { start.byte } else { 0 };
        let to = if line == end.line { end.byte } else { len };
        (from < to).then(|| from.min(len)..to.min(len))
    }

    pub(super) fn text(self, lines: &[SharedString]) -> String {
        let (start, end) = self.ordered();
        lines
            .iter()
            .enumerate()
            .skip(start.line)
            .take(end.line.saturating_sub(start.line) + 1)
            .map(|(index, line)| {
                self.range_on(index, line.len())
                    .map_or("", |range| &line[range])
            })
            .collect::<Vec<_>>()
            .join(LINE_BREAK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn point(line: usize, byte: usize) -> TextPoint {
        TextPoint { line, byte }
    }

    fn lines() -> [SharedString; 3] {
        ["fn main() {", "    run();", "}"].map(SharedString::from)
    }

    #[test]
    fn a_drag_within_one_line_selects_between_its_ends_either_way() {
        let forward = CodeSelection::at(point(0, 3)).extended_to(point(0, 7));
        let backward = CodeSelection::at(point(0, 7)).extended_to(point(0, 3));
        assert_eq!(forward.text(&lines()), "main");
        assert_eq!(backward.text(&lines()), "main");
    }

    #[test]
    fn a_drag_across_lines_takes_the_tail_the_middle_and_the_head() {
        let selection = CodeSelection::at(point(2, 1)).extended_to(point(0, 10));
        assert_eq!(selection.text(&lines()), "{\n    run();\n}");
        assert_eq!(selection.range_on(1, 10), Some(0..10));
        assert_eq!(selection.range_on(2, 1), Some(0..1));
    }

    #[test]
    fn a_click_without_a_drag_selects_nothing() {
        let selection = CodeSelection::at(point(1, 4));
        assert_eq!(selection.range_on(1, 10), None);
        assert_eq!(selection.text(&lines()), "");
    }
}
