use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, relative, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::TerminalId;
use krad_repo::EntryKind;

use super::super::suggest::{entry_labels, SuggestKind, Suggestion};
use super::super::EnvironmentShell;
use crate::components::{icon, IconName};
use crate::theme::{
    palette, popover_fill, popover_shadow, Palette, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H,
    RADIUS_MD, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};

const ROW_PREFIX: &str = "chat-suggestion";
const MENU_ID: &str = "chat-suggestions";
const MENU_GAP: f32 = 6.0;
const MENU_MAX_H: f32 = 300.0;
const LOADING_TEXT: &str = "Loading…";
const NO_FILES_TEXT: &str = "No matching files or folders";
const NO_SKILLS_TEXT: &str = "No matching skills";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn suggestion_menu(
        &self,
        composer: TerminalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let suggest = self
            .chat
            .suggest
            .as_ref()
            .filter(|suggest| suggest.composer == composer)?;
        let pal = palette(cx);
        let selected = suggest.selected;
        let empty_text = match suggest.kind {
            SuggestKind::Mention => NO_FILES_TEXT,
            SuggestKind::Skill => NO_SKILLS_TEXT,
        };
        let rows: Vec<AnyElement> = match self.suggestions() {
            None => vec![note(LOADING_TEXT, pal)],
            Some(rows) if rows.is_empty() => vec![note(empty_text, pal)],
            Some(rows) => rows
                .into_iter()
                .enumerate()
                .map(|(index, row)| Self::suggestion_row(index, &row, index == selected, cx))
                .collect(),
        };
        Some(
            div()
                .id(MENU_ID)
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(relative(1.0))
                .mb(px(MENU_GAP))
                .max_h(px(MENU_MAX_H))
                .overflow_y_scroll()
                .track_scroll(&self.chat.suggest_scroll)
                .flex()
                .flex_col()
                .p(px(SPACE_1))
                .rounded(px(RADIUS_MD))
                .bg(popover_fill(pal, cx))
                .border_1()
                .border_color(pal.border)
                .shadow(popover_shadow(pal, cx))
                .occlude()
                .children(rows)
                .into_any_element(),
        )
    }

    fn suggestion_row(
        index: usize,
        row: &Suggestion,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let (glyph, name, detail) = match row {
            Suggestion::Entry(entry) => {
                let (name, folder) = entry_labels(entry);
                let glyph = match entry.kind {
                    EntryKind::Directory => IconName::Folder,
                    EntryKind::File => IconName::File,
                };
                (glyph, name, folder)
            }
            Suggestion::Skill(skill) => (
                IconName::Magic,
                skill.name.clone(),
                skill.description.clone(),
            ),
        };
        div()
            .id((ROW_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .when(selected, |row| row.bg(pal.bg_selected))
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.accept_suggestion(index, window, cx);
            }))
            .child(icon(glyph, ICON_SM, pal.text_muted))
            .child(
                div()
                    .flex_none()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_strong))
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(detail),
            )
            .into_any_element()
    }
}

fn note(text: &'static str, pal: &Palette) -> AnyElement {
    div()
        .px(px(SPACE_2))
        .py(px(SPACE_1_5))
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_muted))
        .child(text)
        .into_any_element()
}
