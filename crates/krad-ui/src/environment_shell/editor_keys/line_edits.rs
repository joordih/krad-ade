use gpui_kit::component::input::{Rope, RopeExt as _};
use krad_lsp::lsp_types::{Position, Range, TextEdit};

const NEWLINE: &str = "\n";
const CARRIAGE_RETURN: char = '\r';
const COMMENT_GAP: &str = " ";

pub(super) struct LineEdit {
    pub(super) edits: Vec<TextEdit>,
    pub(super) cursor: Position,
}

fn line_text(text: &Rope, line: u32) -> String {
    let mut found = text.slice_line(line as usize).to_string();
    if found.ends_with(CARRIAGE_RETURN) {
        found.pop();
    }
    found
}

fn chars(text: &str) -> u32 {
    u32::try_from(text.chars().count()).unwrap_or(u32::MAX)
}

fn last_line(text: &Rope) -> u32 {
    u32::try_from(text.lines_len().saturating_sub(1)).unwrap_or(u32::MAX)
}

fn edit(start: Position, end: Position, new_text: String) -> TextEdit {
    TextEdit::new(Range::new(start, end), new_text)
}

pub(super) fn duplicate_line(text: &Rope, cursor: Position) -> LineEdit {
    let line = line_text(text, cursor.line);
    LineEdit {
        edits: vec![edit(
            Position::new(cursor.line, 0),
            Position::new(cursor.line, 0),
            format!("{line}{NEWLINE}"),
        )],
        cursor: Position::new(cursor.line + 1, cursor.character),
    }
}

pub(super) fn delete_line(text: &Rope, cursor: Position) -> LineEdit {
    let row = cursor.line;
    let last = last_line(text);
    let (start, end, landing) = if row < last {
        (Position::new(row, 0), Position::new(row + 1, 0), row)
    } else if row > 0 {
        let above = chars(&line_text(text, row - 1));
        let end = chars(&line_text(text, row));
        (
            Position::new(row - 1, above),
            Position::new(row, end),
            row - 1,
        )
    } else {
        (
            Position::new(0, 0),
            Position::new(0, chars(&line_text(text, 0))),
            0,
        )
    };
    LineEdit {
        edits: vec![edit(start, end, String::new())],
        cursor: Position::new(landing, cursor.character),
    }
}

pub(super) fn move_line(text: &Rope, cursor: Position, up: bool) -> Option<LineEdit> {
    let row = cursor.line;
    let other = if up {
        row.checked_sub(1)?
    } else {
        Some(row + 1).filter(|next| *next <= last_line(text))?
    };
    let (first, second) = (row.min(other), row.max(other));
    let (upper, lower) = (line_text(text, first), line_text(text, second));
    Some(LineEdit {
        edits: vec![edit(
            Position::new(first, 0),
            Position::new(second, chars(&lower)),
            format!("{lower}{NEWLINE}{upper}"),
        )],
        cursor: Position::new(other, cursor.character),
    })
}

pub(super) fn toggle_comment(text: &Rope, cursor: Position, prefix: &str) -> LineEdit {
    let row = cursor.line;
    let line = line_text(text, row);
    let rest = line.trim_start();
    let indent = chars(&line) - chars(rest);
    if let Some(after) = rest.strip_prefix(prefix) {
        let removed = chars(prefix) + u32::from(after.starts_with(COMMENT_GAP));
        return LineEdit {
            edits: vec![edit(
                Position::new(row, indent),
                Position::new(row, indent + removed),
                String::new(),
            )],
            cursor: Position::new(row, cursor.character.saturating_sub(removed)),
        };
    }
    let added = format!("{prefix}{COMMENT_GAP}");
    LineEdit {
        cursor: Position::new(row, cursor.character + chars(&added)),
        edits: vec![edit(
            Position::new(row, indent),
            Position::new(row, indent),
            added,
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(source: &str, change: &LineEdit) -> String {
        let mut text = Rope::from(source);
        for edit in &change.edits {
            let start = text.position_to_offset(&edit.range.start);
            let end = text.position_to_offset(&edit.range.end);
            text.replace(start..end, &edit.new_text);
        }
        text.to_string()
    }

    const SOURCE: &str = "one\ntwo\nthree";

    #[test]
    fn a_duplicated_line_lands_below_with_the_caret() {
        let change = duplicate_line(&Rope::from(SOURCE), Position::new(1, 2));
        assert_eq!(apply(SOURCE, &change), "one\ntwo\ntwo\nthree");
        assert_eq!(change.cursor, Position::new(2, 2));
    }

    #[test]
    fn deleting_takes_the_whole_line_even_the_last() {
        let middle = delete_line(&Rope::from(SOURCE), Position::new(1, 0));
        assert_eq!(apply(SOURCE, &middle), "one\nthree");
        let last = delete_line(&Rope::from(SOURCE), Position::new(2, 0));
        assert_eq!(apply(SOURCE, &last), "one\ntwo");
        assert_eq!(last.cursor.line, 1);
    }

    #[test]
    fn a_line_swaps_with_its_neighbour_but_not_past_the_edges() {
        let text = Rope::from(SOURCE);
        let Some(down) = move_line(&text, Position::new(0, 1), false) else {
            unreachable!("the first line can move down");
        };
        assert_eq!(apply(SOURCE, &down), "two\none\nthree");
        assert_eq!(down.cursor, Position::new(1, 1));
        assert!(move_line(&text, Position::new(0, 0), true).is_none());
        assert!(move_line(&text, Position::new(2, 0), false).is_none());
    }

    #[test]
    fn commenting_toggles_after_the_indent() {
        let source = "    let a = 1;";
        let commented = toggle_comment(&Rope::from(source), Position::new(0, 8), "//");
        let once = apply(source, &commented);
        assert_eq!(once, "    // let a = 1;");
        assert_eq!(commented.cursor, Position::new(0, 11));
        let back = toggle_comment(&Rope::from(once.as_str()), Position::new(0, 11), "//");
        assert_eq!(apply(&once, &back), source);
    }
}
