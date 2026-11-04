use gpui_kit::component::input::{Rope, RopeExt as _};
use krad_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, InsertTextFormat,
    MarkupContent, Range, TextEdit,
};
use std::cmp::Ordering;

use super::super::snippet::plain_snippet;
use crate::theme::Palette;

const MAX_SHOWN: usize = 200;

pub(in crate::environment_shell::editor_popovers) fn word_start(
    text: &Rope,
    offset: usize,
) -> usize {
    let line = text.offset_to_point(offset).row;
    let line_start = text.line_start_offset(line);
    let before = text.slice(line_start..offset).to_string();
    let word = before
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
        .last()
        .map_or(before.len(), |(index, _)| index);
    line_start + word
}

fn key_of(item: &CompletionItem) -> &str {
    item.filter_text.as_deref().unwrap_or(&item.label)
}

pub(in crate::environment_shell::editor_popovers) fn matching(
    items: Vec<CompletionItem>,
    typed: &str,
) -> Vec<CompletionItem> {
    let typed = typed.to_lowercase();
    let mut kept: Vec<(bool, CompletionItem)> = items
        .into_iter()
        .filter_map(|item| {
            let key = key_of(&item).to_lowercase();
            if key.starts_with(&typed) {
                Some((true, item))
            } else if key.contains(&typed) {
                Some((false, item))
            } else {
                None
            }
        })
        .collect();
    kept.sort_by(|(left_prefix, left), (right_prefix, right)| {
        right_prefix
            .cmp(left_prefix)
            .then_with(|| match (&left.sort_text, &right.sort_text) {
                (Some(left), Some(right)) => left.cmp(right),
                _ => Ordering::Equal,
            })
            .then_with(|| left.label.len().cmp(&right.label.len()))
    });
    kept.into_iter()
        .take(MAX_SHOWN)
        .map(|(_, item)| item)
        .collect()
}

pub(super) const fn kind_badge(kind: Option<CompletionItemKind>) -> &'static str {
    match kind {
        Some(
            CompletionItemKind::METHOD
            | CompletionItemKind::FUNCTION
            | CompletionItemKind::CONSTRUCTOR,
        ) => "m",
        Some(CompletionItemKind::FIELD | CompletionItemKind::PROPERTY) => "f",
        Some(CompletionItemKind::VARIABLE | CompletionItemKind::CONSTANT) => "v",
        Some(CompletionItemKind::CLASS | CompletionItemKind::STRUCT) => "c",
        Some(CompletionItemKind::INTERFACE) => "i",
        Some(CompletionItemKind::ENUM | CompletionItemKind::ENUM_MEMBER) => "e",
        Some(CompletionItemKind::MODULE) => "p",
        Some(CompletionItemKind::KEYWORD) => "k",
        Some(CompletionItemKind::SNIPPET) => "s",
        _ => "·",
    }
}

pub(super) fn badge_color(kind: Option<CompletionItemKind>, pal: &Palette) -> u32 {
    match kind {
        Some(
            CompletionItemKind::METHOD
            | CompletionItemKind::FUNCTION
            | CompletionItemKind::CONSTRUCTOR,
        ) => pal.syntax.keyword,
        Some(
            CompletionItemKind::CLASS
            | CompletionItemKind::STRUCT
            | CompletionItemKind::INTERFACE
            | CompletionItemKind::ENUM,
        ) => pal.syntax.type_name,
        Some(
            CompletionItemKind::FIELD
            | CompletionItemKind::PROPERTY
            | CompletionItemKind::VARIABLE
            | CompletionItemKind::CONSTANT,
        ) => pal.syntax.number,
        _ => pal.text_muted,
    }
}

pub(super) fn documentation(item: &CompletionItem) -> Option<String> {
    match item.documentation.as_ref()? {
        Documentation::String(text) => Some(text.clone()),
        Documentation::MarkupContent(MarkupContent { value, .. }) => Some(value.clone()),
    }
    .filter(|text| !text.trim().is_empty())
}

pub(super) fn accepted_edit(
    item: &CompletionItem,
    text: &Rope,
    start: usize,
    cursor: usize,
) -> (TextEdit, usize) {
    let (range, raw) = match &item.text_edit {
        Some(CompletionTextEdit::Edit(edit)) => (edit.range, edit.new_text.clone()),
        Some(CompletionTextEdit::InsertAndReplace(edit)) => (edit.replace, edit.new_text.clone()),
        None => (
            Range::new(
                text.offset_to_position(start),
                text.offset_to_position(cursor),
            ),
            item.insert_text
                .clone()
                .unwrap_or_else(|| item.label.clone()),
        ),
    };
    let (inserted, caret) = if item.insert_text_format == Some(InsertTextFormat::SNIPPET) {
        let plain = plain_snippet(&raw);
        (plain.text, plain.cursor)
    } else {
        let length = raw.len();
        (raw, length)
    };
    let from = text.position_to_offset(&range.start);
    (TextEdit::new(range, inserted), from + caret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, sort: &str) -> CompletionItem {
        CompletionItem {
            label: label.to_owned(),
            sort_text: Some(sort.to_owned()),
            ..CompletionItem::default()
        }
    }

    #[test]
    fn items_starting_with_the_typed_word_come_first_in_server_order() {
        let shown: Vec<String> = matching(
            vec![
                item("getName", "2"),
                item("isNamed", "1"),
                item("getNameLength", "1"),
                item("other", "0"),
            ],
            "getn",
        )
        .into_iter()
        .map(|item| item.label)
        .collect();
        assert_eq!(shown, ["getNameLength", "getName"]);
        let inside: Vec<String> =
            matching(vec![item("getName", "2"), item("isNamed", "1")], "name")
                .into_iter()
                .map(|item| item.label)
                .collect();
        assert_eq!(inside, ["isNamed", "getName"]);
    }

    #[test]
    fn the_word_under_the_cursor_starts_after_the_last_separator() {
        let text = Rope::from("  logger.inf");
        assert_eq!(word_start(&text, 12), 9);
        assert_eq!(word_start(&text, 9), 9);
    }

    #[test]
    fn an_accepted_snippet_replaces_the_typed_word_and_leaves_the_cursor_in_its_first_stop() {
        let text = Rope::from("log.inf");
        let snippet = CompletionItem {
            label: "info".to_owned(),
            insert_text: Some("info(${1:msg})".to_owned()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        };
        let (edit, caret) = accepted_edit(&snippet, &text, 4, 7);
        assert_eq!(edit.new_text, "info(msg)");
        assert_eq!(
            edit.range,
            Range::new(text.offset_to_position(4), text.offset_to_position(7))
        );
        assert_eq!(caret, 9);
    }
}
