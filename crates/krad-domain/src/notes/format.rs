use std::ops::Range;

const BOLD: &str = "**";
const ITALIC: &str = "*";
const STRIKETHROUGH: &str = "~~";
const HIGHLIGHT: &str = "==";
const INLINE_CODE: &str = "`";
const INLINE_MATH: &str = "$";
const HEADING_MARK: char = '#';
const BULLET: &str = "- ";
const TASK: &str = "- [ ] ";
const QUOTE: &str = "> ";
const NUMBER_SUFFIX: &str = ". ";
const LINK_URL: &str = "url";
const LINK_TEXT: &str = "text";
const CODE_FENCE: &str = "```";
const MATH_FENCE: &str = "$$";
const TABLE_HEADER: &str = "Column";
const CALLOUT_OPEN: &str = "> [!note] ";
const CALLOUT_TITLE: &str = "Title";
const DIVIDER: &str = "---";
const NEWLINE: char = '\n';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteFormat {
    Bold,
    Italic,
    Strikethrough,
    Highlight,
    InlineCode,
    InlineMath,
    Link,
    Heading1,
    Heading2,
    Heading3,
    BulletList,
    NumberedList,
    TaskList,
    Quote,
    CodeBlock,
    MathBlock,
    Table,
    Callout,
    Divider,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatEdit {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

#[must_use]
pub fn format_edit(text: &str, selection: Range<usize>, format: NoteFormat) -> FormatEdit {
    let selection = clamp(text, selection);
    match format {
        NoteFormat::Bold => wrap(text, selection, BOLD),
        NoteFormat::Italic => wrap(text, selection, ITALIC),
        NoteFormat::Strikethrough => wrap(text, selection, STRIKETHROUGH),
        NoteFormat::Highlight => wrap(text, selection, HIGHLIGHT),
        NoteFormat::InlineCode => wrap(text, selection, INLINE_CODE),
        NoteFormat::InlineMath => wrap(text, selection, INLINE_MATH),
        NoteFormat::Link => link(text, selection),
        NoteFormat::Heading1 => heading(text, selection, 1),
        NoteFormat::Heading2 => heading(text, selection, 2),
        NoteFormat::Heading3 => heading(text, selection, 3),
        NoteFormat::BulletList => prefix_lines(text, selection, |_| BULLET.to_owned()),
        NoteFormat::TaskList => prefix_lines(text, selection, |_| TASK.to_owned()),
        NoteFormat::Quote => prefix_lines(text, selection, |_| QUOTE.to_owned()),
        NoteFormat::NumberedList => {
            prefix_lines(text, selection, |nth| format!("{}{NUMBER_SUFFIX}", nth + 1))
        }
        NoteFormat::CodeBlock => fenced(text, selection, CODE_FENCE),
        NoteFormat::MathBlock => fenced(text, selection, MATH_FENCE),
        NoteFormat::Table => table(text, selection),
        NoteFormat::Callout => callout(text, selection),
        NoteFormat::Divider => block(text, selection, DIVIDER, None),
    }
}

fn clamp(text: &str, selection: Range<usize>) -> Range<usize> {
    let fit = |mut at: usize| {
        at = at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let (start, end) = (fit(selection.start), fit(selection.end));
    start.min(end)..start.max(end)
}

fn wrap(text: &str, selection: Range<usize>, mark: &str) -> FormatEdit {
    let len = mark.len();
    let before = selection
        .start
        .checked_sub(len)
        .map(|at| at..selection.start);
    let after = selection.end..selection.end + len;
    let wrapped = !selection.is_empty()
        && before
            .clone()
            .is_some_and(|before| text.get(before) == Some(mark))
        && text.get(after.clone()) == Some(mark);
    let inner = &text[selection.clone()];
    if let (true, Some(before)) = (wrapped, before) {
        return FormatEdit {
            range: before.start..after.end,
            text: inner.to_owned(),
            selection: 0..inner.len(),
        };
    }
    FormatEdit {
        range: selection,
        text: format!("{mark}{inner}{mark}"),
        selection: len..len + inner.len(),
    }
}

fn link(text: &str, selection: Range<usize>) -> FormatEdit {
    let inner = &text[selection.clone()];
    let shown = if inner.is_empty() { LINK_TEXT } else { inner };
    let url_at = "[".len() + shown.len() + "](".len();
    let selection_after = if inner.is_empty() {
        1..1 + shown.len()
    } else {
        url_at..url_at + LINK_URL.len()
    };
    FormatEdit {
        range: selection,
        text: format!("[{shown}]({LINK_URL})"),
        selection: selection_after,
    }
}

fn lines_of(text: &str, selection: &Range<usize>) -> Range<usize> {
    let start = text[..selection.start]
        .rfind(NEWLINE)
        .map_or(0, |at| at + 1);
    let end = text[selection.end..]
        .find(NEWLINE)
        .map_or(text.len(), |at| selection.end + at);
    start..end
}

fn prefix_lines(
    text: &str,
    selection: Range<usize>,
    prefix: impl Fn(usize) -> String,
) -> FormatEdit {
    let range = lines_of(text, &selection);
    let lines: Vec<&str> = text[range.clone()].split(NEWLINE).collect();
    let prefixed = lines
        .iter()
        .enumerate()
        .all(|(nth, line)| line.starts_with(&prefix(nth)));
    let changed: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(nth, line)| {
            let mark = prefix(nth);
            if prefixed {
                line[mark.len()..].to_owned()
            } else {
                format!("{mark}{line}")
            }
        })
        .collect();
    let changed = changed.join("\n");
    let len = changed.len();
    FormatEdit {
        range,
        text: changed,
        selection: len..len,
    }
}

fn heading(text: &str, selection: Range<usize>, level: usize) -> FormatEdit {
    let range = lines_of(text, &selection);
    let mark = format!("{} ", HEADING_MARK.to_string().repeat(level));
    let changed: Vec<String> = text[range.clone()]
        .split(NEWLINE)
        .map(|line| {
            let bare = line.trim_start_matches(HEADING_MARK);
            let had = line.len() - bare.len();
            let bare = bare.strip_prefix(' ').unwrap_or(bare);
            if had == level {
                bare.to_owned()
            } else {
                format!("{mark}{bare}")
            }
        })
        .collect();
    let changed = changed.join("\n");
    let len = changed.len();
    FormatEdit {
        range,
        text: changed,
        selection: len..len,
    }
}

fn block(
    text: &str,
    selection: Range<usize>,
    body: &str,
    inside: Option<Range<usize>>,
) -> FormatEdit {
    let lead = if selection.start == 0 || text[..selection.start].ends_with(NEWLINE) {
        ""
    } else {
        "\n"
    };
    let trail = if text[selection.end..].starts_with(NEWLINE) {
        ""
    } else {
        "\n"
    };
    let at = lead.len();
    FormatEdit {
        range: selection,
        text: format!("{lead}{body}{trail}"),
        selection: inside.map_or_else(
            || {
                let end = at + body.len();
                end..end
            },
            |inside| at + inside.start..at + inside.end,
        ),
    }
}

fn fenced(text: &str, selection: Range<usize>, fence: &str) -> FormatEdit {
    let inner = &text[selection.clone()];
    let body = format!("{fence}\n{inner}\n{fence}");
    let start = fence.len() + 1;
    block(text, selection, &body, Some(start..start + inner.len()))
}

fn table(text: &str, selection: Range<usize>) -> FormatEdit {
    let body = format!("| {TABLE_HEADER} | {TABLE_HEADER} |\n| --- | --- |\n|  |  |");
    block(text, selection, &body, Some(2..2 + TABLE_HEADER.len()))
}

fn callout(text: &str, selection: Range<usize>) -> FormatEdit {
    let inner = &text[selection.clone()];
    let quoted: Vec<String> = inner
        .split(NEWLINE)
        .map(|line| format!("{QUOTE}{line}"))
        .collect();
    let body = format!("{CALLOUT_OPEN}{CALLOUT_TITLE}\n{}", quoted.join("\n"));
    let title = CALLOUT_OPEN.len();
    block(
        text,
        selection,
        &body,
        Some(title..title + CALLOUT_TITLE.len()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, selection: Range<usize>, format: NoteFormat) -> (String, String) {
        let edit = format_edit(text, selection, format);
        let mut changed = text.to_owned();
        changed.replace_range(edit.range.clone(), &edit.text);
        let start = edit.range.start;
        let selected = changed[start + edit.selection.start..start + edit.selection.end].to_owned();
        (changed, selected)
    }

    #[test]
    fn marks_wrap_the_selection_and_come_off_again() {
        let (bold, selected) = apply("a word here", 2..6, NoteFormat::Bold);
        assert_eq!(bold, "a **word** here");
        assert_eq!(selected, "word");
        let (plain, selected) = apply(&bold, 4..8, NoteFormat::Bold);
        assert_eq!(plain, "a word here");
        assert_eq!(selected, "word");
    }

    #[test]
    fn marks_without_a_selection_wait_for_what_comes_next() {
        let edit = format_edit("ab", 1..1, NoteFormat::Italic);
        assert_eq!(edit.text, "**");
        assert_eq!(edit.selection, 1..1);
    }

    #[test]
    fn lines_take_and_drop_their_prefix() {
        let (listed, _) = apply("one\ntwo\nthree", 1..6, NoteFormat::BulletList);
        assert_eq!(listed, "- one\n- two\nthree");
        let (back, _) = apply(&listed, 2..8, NoteFormat::BulletList);
        assert_eq!(back, "one\ntwo\nthree");
        let (numbered, _) = apply("a\nb", 0..3, NoteFormat::NumberedList);
        assert_eq!(numbered, "1. a\n2. b");
    }

    #[test]
    fn headings_switch_level_and_toggle_off() {
        let (h2, _) = apply("# Title", 3..3, NoteFormat::Heading2);
        assert_eq!(h2, "## Title");
        let (plain, _) = apply(&h2, 0..0, NoteFormat::Heading2);
        assert_eq!(plain, "Title");
    }

    #[test]
    fn blocks_sit_on_lines_of_their_own() {
        let (code, selected) = apply("see fn x", 4..8, NoteFormat::CodeBlock);
        assert_eq!(code, "see \n```\nfn x\n```\n");
        assert_eq!(selected, "fn x");
        let (table, selected) = apply("", 0..0, NoteFormat::Table);
        assert!(table.starts_with("| Column | Column |\n| --- |"));
        assert_eq!(selected, "Column");
        let (callout, selected) = apply("note", 0..4, NoteFormat::Callout);
        assert_eq!(callout, "> [!note] Title\n> note\n");
        assert_eq!(selected, "Title");
    }

    #[test]
    fn links_select_what_is_left_to_fill() {
        let (linked, selected) = apply("go", 0..2, NoteFormat::Link);
        assert_eq!(linked, "[go](url)");
        assert_eq!(selected, "url");
        let (empty, selected) = apply("", 0..0, NoteFormat::Link);
        assert_eq!(empty, "[text](url)");
        assert_eq!(selected, "text");
    }

    #[test]
    fn selections_off_the_text_are_clamped() {
        let edit = format_edit("é", 1..9, NoteFormat::Bold);
        assert_eq!(edit.range, 0..2);
    }
}
