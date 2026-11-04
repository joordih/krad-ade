mod items;

use gpui_kit::component::input::{EditorState, RopeExt as _};
use gpui_kit::component::text::TextView;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_lsp::lsp_types::CompletionItem;

use super::super::language_servers::Document;
use super::super::EnvironmentShell;
use super::{place, placed, popover_panel, EditorPopover};
use crate::theme::{palette, Palette, LINE_SM, LINE_XS, SPACE_1, SPACE_2, TEXT_SM, TEXT_XS};
use items::{accepted_edit, badge_color, documentation, kind_badge, matching, word_start};

const MENU_W: f32 = 420.0;
const DOCS_W: f32 = 340.0;
const MENU_MAX_H: f32 = 300.0;
const ROW_H: f32 = 24.0;
const ROW_RADIUS: f32 = 6.0;
const BADGE_W: f32 = 18.0;
const MENU_ID: &str = "editor-completion";
const DOCS_ID: &str = "editor-completion-docs";
const ROW_PREFIX: &str = "editor-completion-row";

pub(in crate::environment_shell) struct CompletionMenu {
    pub(super) tab: krad_domain::TerminalId,
    start: usize,
    items: Vec<CompletionItem>,
    selected: usize,
}

impl CompletionMenu {
    fn step(&mut self, forward: bool) {
        let count = self.items.len();
        if count > 0 {
            self.selected = if forward {
                (self.selected + 1) % count
            } else {
                (self.selected + count - 1) % count
            };
        }
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn show_completions(
        &mut self,
        document: &Document,
        cursor: usize,
        items: Vec<CompletionItem>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_showing(document) else {
            return;
        };
        let Some((state, _)) = self.popover_editor(tab) else {
            return;
        };
        let text = state.read(cx).text();
        let start = word_start(text, cursor);
        let typed = text.slice(start..cursor).to_string();
        let items = matching(items, &typed);
        if items.is_empty() {
            self.close_completions(cx);
            return;
        }
        self.editor_popover = Some(EditorPopover::Completion(CompletionMenu {
            tab,
            start,
            items,
            selected: 0,
        }));
        cx.notify();
    }

    pub(in crate::environment_shell) fn editor_text_changed(
        &mut self,
        state: &Entity<EditorState>,
        cx: &mut Context<Self>,
    ) {
        let Some(EditorPopover::Completion(menu)) = &mut self.editor_popover else {
            self.close_editor_popovers(cx);
            return;
        };
        let editor = state.read(cx);
        let cursor = editor.cursor();
        let text = editor.text();
        if cursor <= menu.start || word_start(text, cursor) != menu.start {
            self.close_completions(cx);
            return;
        }
        let typed = text.slice(menu.start..cursor).to_string();
        let items = matching(std::mem::take(&mut menu.items), &typed);
        if items.is_empty() {
            self.close_completions(cx);
            return;
        }
        menu.items = items;
        menu.selected = 0;
        cx.notify();
    }

    pub(in crate::environment_shell) fn completions_open(&self) -> bool {
        matches!(self.editor_popover, Some(EditorPopover::Completion(_)))
    }

    pub(super) fn close_completions(&mut self, cx: &mut Context<Self>) {
        if self.completions_open() {
            self.editor_popover = None;
            cx.notify();
        }
    }

    pub(in crate::environment_shell) fn step_completion(
        &mut self,
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(EditorPopover::Completion(menu)) = &mut self.editor_popover {
            menu.step(forward);
            cx.notify();
        }
    }

    pub(in crate::environment_shell) fn accept_completion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(EditorPopover::Completion(menu)) = self.editor_popover.take() else {
            return false;
        };
        cx.notify();
        let Some(item) = menu.items.get(menu.selected) else {
            return true;
        };
        let Some((state, _)) = self.popover_editor(menu.tab) else {
            return true;
        };
        let item = item.clone();
        state.update(cx, |state: &mut EditorState, cx| {
            let cursor = state.cursor();
            let (edit, caret) = accepted_edit(&item, state.text(), menu.start, cursor);
            state.apply_lsp_edits(&vec![edit], window, cx);
            let caret = state
                .text()
                .offset_to_position(caret.min(state.text().len()));
            state.set_cursor_position(caret, window, cx);
            if let Some(extra) = item
                .additional_text_edits
                .as_ref()
                .filter(|extra| !extra.is_empty())
            {
                state.apply_lsp_edits(extra, window, cx);
            }
        });
        true
    }

    fn completion_row(
        index: usize,
        item: &CompletionItem,
        selected: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id((ROW_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(ROW_H))
            .px(px(SPACE_1))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(selected, |row| row.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                if let Some(EditorPopover::Completion(menu)) = &mut this.editor_popover {
                    menu.selected = index;
                }
                this.accept_completion(window, cx);
            }))
            .child(
                div()
                    .flex_none()
                    .w(px(BADGE_W))
                    .text_size(px(TEXT_XS))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(badge_color(item.kind, pal)))
                    .child(kind_badge(item.kind)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_strong))
                    .child(SharedString::from(item.label.clone())),
            )
            .children(item.detail.clone().map(|detail| {
                div()
                    .flex_none()
                    .max_w(px(MENU_W / 2.0))
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(detail)
            }))
            .into_any_element()
    }

    pub(super) fn completion_overlay(
        &self,
        menu: &CompletionMenu,
        state: &Entity<EditorState>,
        root: (f32, f32),
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let anchor = Self::anchor_bounds(state, &(menu.start..menu.start), cx)?;
        let pal = palette(cx);
        let placement = place(
            anchor,
            self.frame_inset,
            root,
            (MENU_W + DOCS_W, MENU_MAX_H),
        );
        let rows: Vec<AnyElement> = menu
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| Self::completion_row(index, item, index == menu.selected, pal, cx))
            .collect();
        let docs = menu
            .items
            .get(menu.selected)
            .and_then(documentation)
            .map(|docs| {
                popover_panel(pal, cx)
                    .relative()
                    .id(DOCS_ID)
                    .w(px(DOCS_W))
                    .max_h(px(placement.max_height))
                    .overflow_y_scroll()
                    .text_size(px(TEXT_SM))
                    .child(TextView::markdown(
                        ElementId::from(DOCS_ID),
                        SharedString::from(docs),
                    ))
            });
        Some(
            placed(
                div()
                    .absolute()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(SPACE_1)),
                placement,
            )
            .occlude()
            .child(
                popover_panel(pal, cx)
                    .relative()
                    .id(MENU_ID)
                    .w(px(MENU_W))
                    .max_h(px(placement.max_height))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .children(docs)
            .into_any_element(),
        )
    }
}
