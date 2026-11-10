use gpui_kit::{
    div, px, rgb, AnyElement, Context, Div, FontWeight, HighlightStyle, InteractiveElement,
    IntoElement, ParentElement, Stateful, StatefulInteractiveElement, StrikethroughStyle, Styled,
    StyledText,
};
use krad_lsp::lsp_types::Position;
use krad_repo::{FileMatches, FoundLine};
use std::ops::Range;
use std::path::Path;

use super::super::super::EnvironmentShell;
use super::preview::{line_preview, Mark, SearchRow};
use super::SearchResults;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, tinted, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1_5, SPACE_2, TEXT_SM,
    TEXT_XS,
};

const RESULT_PREFIX: &str = "side-search-row";
const REPLACE_FILE_PREFIX: &str = "side-search-replace-file";
const TOOLTIP_REPLACE_FILE: &str = "Replace in this file";
const ROW_H: f32 = 24.0;
const MATCH_INDENT: f32 = 22.0;
const LINE_NUMBER_W: f32 = 34.0;
const MATCH_WASH_ALPHA: u32 = 0x40;
const STRIKE_THICKNESS: f32 = 1.0;
const LINE_BASE: u32 = 1;

impl EnvironmentShell {
    pub(super) fn side_search_rows(
        &self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let SearchResults::Found { matches, rows } = &self.side_search.results else {
            return Vec::new();
        };
        rows.get(range.clone())
            .unwrap_or_default()
            .iter()
            .zip(range)
            .filter_map(|(row, index)| match *row {
                SearchRow::File(file) => {
                    let file = matches.files.get(file)?;
                    Some(self.search_file_row(file, index, cx))
                }
                SearchRow::Line { file, line } => {
                    let file = matches.files.get(file)?;
                    let found = file.lines.get(line)?;
                    Some(self.search_line_row(&file.path, found, index, cx))
                }
            })
            .collect()
    }

    fn search_file_row(
        &self,
        file: &FileMatches,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let path = file.path.clone();
        let name = file
            .path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned());
        let folder = file
            .path
            .parent()
            .map_or_default(|parent| self.relative_to_root(parent).display().to_string());
        let mut row = search_row_base(index, pal)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_file_at(&path, None, cx);
            }))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .child(icon(IconName::File, ICON_SM, pal.text_muted))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(folder),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(file.match_count().to_string()),
            );
        if self.side_search.replace_drawer.is_open() && !self.side_search.replacing {
            let path = file.path.clone();
            row = row.child(
                Button::new((REPLACE_FILE_PREFIX, index))
                    .icon(IconName::Repeat)
                    .small()
                    .tooltip(TOOLTIP_REPLACE_FILE)
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.replace_in_found_file(&path, cx);
                    })),
            );
        }
        row.into_any_element()
    }

    fn search_line_row(
        &self,
        path: &Path,
        found: &FoundLine,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let preview = line_preview(found, self.side_search.replacement());
        let highlights: Vec<(Range<usize>, HighlightStyle)> = preview
            .marks
            .iter()
            .map(|(range, mark)| (range.clone(), mark_style(*mark, pal)))
            .collect();
        let path = path.to_path_buf();
        let at = cursor_at(found);
        search_row_base(index, pal)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.open_file_at(&path, Some(at), cx);
            }))
            .pl(px(MATCH_INDENT))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(
                div()
                    .flex_none()
                    .w(px(LINE_NUMBER_W))
                    .text_color(rgb(pal.text_muted))
                    .child(found.line.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(pal.text_default))
                    .child(StyledText::new(preview.text).with_highlights(highlights)),
            )
            .into_any_element()
    }
}

fn search_row_base(index: usize, pal: &Palette) -> Stateful<Div> {
    div()
        .id((RESULT_PREFIX, index))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .w_full()
        .h(px(ROW_H))
        .px(px(SPACE_2))
        .rounded(px(RADIUS_XS))
        .cursor_pointer()
        .hover(|style| style.bg(pal.bg_hover))
}

fn mark_style(mark: Mark, pal: &Palette) -> HighlightStyle {
    match mark {
        Mark::Found => HighlightStyle {
            background_color: Some(tinted(pal.accent, MATCH_WASH_ALPHA).into()),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..HighlightStyle::default()
        },
        Mark::Removed => HighlightStyle {
            background_color: Some(tinted(pal.error_fg, MATCH_WASH_ALPHA).into()),
            strikethrough: Some(StrikethroughStyle {
                thickness: px(STRIKE_THICKNESS),
                color: Some(rgb(pal.error_fg).into()),
            }),
            ..HighlightStyle::default()
        },
        Mark::Inserted => HighlightStyle {
            background_color: Some(tinted(pal.success_fg, MATCH_WASH_ALPHA).into()),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..HighlightStyle::default()
        },
    }
}

fn cursor_at(found: &FoundLine) -> Position {
    let start = found.matches.first().map_or(0, |range| range.start);
    let character = found
        .text
        .get(..start)
        .map_or(0, |before| before.encode_utf16().count());
    Position {
        line: found.line.saturating_sub(LINE_BASE),
        character: u32::try_from(character).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_opens_at_its_first_match_in_utf16() {
        let found = FoundLine {
            line: 3,
            text: "é = foo".to_owned(),
            matches: vec![5..8, 0..2],
        };
        assert_eq!(
            cursor_at(&found),
            Position {
                line: 2,
                character: 4
            }
        );
    }
}
