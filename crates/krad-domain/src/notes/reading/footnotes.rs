use std::ops::Range;

use super::{escape, FENCE_MARKS, FOOTNOTE_BACK_SCHEME, FOOTNOTE_SCHEME};

const FOOTNOTE_OPEN: &str = "[^";
const FOOTNOTE_CLOSE: char = ']';
const FOOTNOTE_DEFINITION_END: &str = "]:";
const BACK_ARROW: &str = "↩";

pub(super) struct LinkedFootnotes {
    pub(super) markdown: String,
    pub(super) definitions: Vec<(String, Range<usize>)>,
    pub(super) marks: Vec<(String, Range<usize>)>,
}

pub(super) fn link_footnotes(markdown: &str) -> LinkedFootnotes {
    let mut out = String::with_capacity(markdown.len());
    let mut definitions = Vec::new();
    let mut marks: Vec<(String, Range<usize>)> = Vec::new();
    let mut fence: Option<&str> = None;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some(mark) = FENCE_MARKS.iter().find(|mark| trimmed.starts_with(**mark)) {
            fence = match fence {
                Some(open) if open == *mark => None,
                Some(open) => Some(open),
                None => Some(mark),
            };
            out.push_str(line);
            continue;
        }
        if fence.is_some() {
            out.push_str(line);
            continue;
        }
        if let Some(id) = line
            .strip_prefix(FOOTNOTE_OPEN)
            .and_then(|rest| rest.split_once(FOOTNOTE_DEFINITION_END))
            .map(|(id, _)| id.to_owned())
        {
            let text =
                line[FOOTNOTE_OPEN.len() + id.len() + FOOTNOTE_DEFINITION_END.len()..].trim();
            let start = out.len();
            let definition = format!(
                "**{id}.** {text} [{BACK_ARROW}]({FOOTNOTE_BACK_SCHEME}{})
",
                escape(&id)
            );
            out.push_str(&definition);
            definitions.push((id, start..out.len()));
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find(FOOTNOTE_OPEN) {
            let after = &rest[open + FOOTNOTE_OPEN.len()..];
            let Some(close) = after.find(FOOTNOTE_CLOSE) else {
                break;
            };
            let id = &after[..close];
            if id.is_empty() || id.contains(char::is_whitespace) {
                out.push_str(&rest[..open + FOOTNOTE_OPEN.len()]);
                rest = after;
                continue;
            }
            out.push_str(&rest[..open]);
            let start = out.len();
            let mark = format!("[\\[{id}\\]]({FOOTNOTE_SCHEME}{})", escape(id));
            out.push_str(&mark);
            if !marks.iter().any(|(known, _)| known == id) {
                marks.push((id.to_owned(), start..out.len()));
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
    }
    LinkedFootnotes {
        markdown: out,
        definitions,
        marks,
    }
}
