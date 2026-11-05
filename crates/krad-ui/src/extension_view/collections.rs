mod stream;
mod table;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, Context, ElementId, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_extensions::api::view::{ItemAction, Node, Selection, ViewEvent};
use krad_extensions::api::NodeKey;
use std::ops::Range;
use std::sync::Arc;

use super::form::{element_id, indexed_id};
use super::layout::badge;
use super::model::{CachedRow, Shown, TreeRow};
use super::style::icon_name;
use super::ExtensionView;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, row_fill, Palette, ICON_SM, LINE_SM, NAV_ROW_H, RADIUS_XS, SPACE_1, SPACE_2, TEXT_SM,
};

const LIST_PREFIX: &str = "ext-list";
const TREE_PREFIX: &str = "ext-tree";
const TABLE_PREFIX: &str = "ext-table";
const STREAM_PREFIX: &str = "ext-stream";
const ROW_PREFIX: &str = "ext-row";
const HEADER_PREFIX: &str = "ext-header";
const ACTION_PREFIX: &str = "ext-action";
const TREE_INDENT: f32 = 14.0;
const COLLECTION_MIN_H: f32 = 120.0;
const LOADING_ROW_TEXT: &str = "…";

pub(super) fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

pub(super) fn visible(range: &Range<usize>) -> Range<u32> {
    index_u32(range.start)..index_u32(range.end)
}

pub(super) fn row_id(prefix: &'static str, owner: &NodeKey, row: &NodeKey) -> ElementId {
    ElementId::NamedChild(
        Arc::new(element_id(prefix, owner)),
        SharedString::from(row.to_string()),
    )
}

impl ExtensionView {
    pub(super) fn list(
        node: &Node,
        count: u32,
        selection: Selection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = node.key.clone();
        let id = element_id(LIST_PREFIX, &key);
        uniform_list(
            id,
            usize::try_from(count).unwrap_or(usize::MAX),
            cx.processor(
                move |this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                    this.list_rows(&key, selection, &range, cx)
                },
            ),
        )
        .flex_1()
        .min_h(px(COLLECTION_MIN_H))
        .into_any_element()
    }

    pub(super) fn request_rows(&mut self, key: &NodeKey, range: &Range<usize>) {
        if let Some(request) = self.model.rows_needed(key, visible(range)) {
            self.send(request);
        }
    }

    fn list_rows(
        &mut self,
        key: &NodeKey,
        selection: Selection,
        range: &Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.request_rows(key, range);
        let pal = palette(cx);
        visible(range)
            .map(|index| match self.model.row(key, index) {
                Some(CachedRow::List(row)) => {
                    let item = row.node.key.clone();
                    let chosen = self.model.is_chosen(key, &item);
                    let body = self.element(&row.node, pal, cx);
                    let actions = Self::item_actions(key, &item, &row.menu, pal, cx);
                    let list = key.clone();
                    div()
                        .id(row_id(ROW_PREFIX, key, &item))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .px(px(SPACE_2))
                        .rounded(px(RADIUS_XS))
                        .when(chosen, |row| row.bg(row_fill(pal, cx)))
                        .hover(|row| row.bg(pal.bg_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(event) = this.model.choose(&list, &item, selection) {
                                this.emit(event);
                                cx.notify();
                            }
                        }))
                        .child(div().flex_1().min_w_0().child(body))
                        .children(actions)
                        .into_any_element()
                }
                _ => loading_row(pal),
            })
            .collect()
    }

    pub(super) fn item_actions(
        owner: &NodeKey,
        item: &NodeKey,
        actions: &[ItemAction],
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if actions.is_empty() {
            return None;
        }
        let buttons = actions.iter().enumerate().map(|(index, entry)| {
            let action = entry.action.clone();
            let target = item.clone();
            let mut button = Button::new(indexed_id(row_id(ACTION_PREFIX, owner, item), index))
                .variant(ButtonVariant::Ghost)
                .small()
                .tooltip(SharedString::from(entry.label.clone()))
                .on_click(cx.listener(move |this, _, _, _| {
                    this.emit(ViewEvent::ItemAction {
                        action: action.clone(),
                        item: target.clone(),
                    });
                }));
            button = match entry.icon {
                Some(glyph) => button.icon(icon_name(glyph)),
                None => button.label(SharedString::from(entry.label.clone())),
            };
            button.into_any_element()
        });
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .gap(px(SPACE_1))
                .text_color(rgb(pal.text_muted))
                .children(buttons)
                .into_any_element(),
        )
    }

    pub(super) fn tree(&self, node: &Node, _pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let key = node.key.clone();
        let count = self.model.tree_rows(&key).len();
        uniform_list(
            element_id(TREE_PREFIX, &key),
            count,
            cx.processor(
                move |this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                    this.tree_rows(&key, range, cx)
                },
            ),
        )
        .flex_1()
        .min_h(px(COLLECTION_MIN_H))
        .into_any_element()
    }

    fn tree_rows(
        &self,
        key: &NodeKey,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        self.model
            .tree_rows(key)
            .get(range)
            .unwrap_or_default()
            .iter()
            .map(|row| Self::tree_row(key, row, pal, cx))
            .collect()
    }

    fn tree_row(
        tree: &NodeKey,
        row: &TreeRow,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chevron = match row.expansion {
            Shown::Open => Some(IconName::ChevronDown),
            Shown::Closed => Some(IconName::ChevronRight),
            Shown::Leaf => None,
        };
        let owner = tree.clone();
        let item = row.key.clone();
        let indent = f32::from(u16::try_from(row.depth).unwrap_or(u16::MAX)) * TREE_INDENT;
        let actions = Self::item_actions(tree, &row.key, &row.actions, pal, cx);
        let menu = Self::item_actions(tree, &row.key, &row.menu, pal, cx);
        div()
            .id(row_id(ROW_PREFIX, tree, &row.key))
            .h(px(NAV_ROW_H))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .pl(px(indent + SPACE_1))
            .pr(px(SPACE_1))
            .rounded(px(RADIUS_XS))
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(event) = this.model.toggle(&owner, &item) {
                    this.emit(event);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex_none()
                    .w(px(ICON_SM))
                    .children(chevron.map(|chevron| icon(chevron, ICON_SM, pal.text_muted))),
            )
            .children(
                row.icon
                    .map(|glyph| icon(icon_name(glyph), ICON_SM, pal.text_subtle)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .gap(px(SPACE_2))
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(rgb(pal.text_default))
                    .child(row.label.clone())
                    .children(
                        row.description
                            .clone()
                            .map(|text| div().text_color(rgb(pal.text_muted)).child(text)),
                    ),
            )
            .children(
                row.badge
                    .as_ref()
                    .map(|found| badge(&found.text, found.tone, pal)),
            )
            .children(actions)
            .children(menu)
            .into_any_element()
    }
}

pub(super) fn loading_row(pal: &Palette) -> AnyElement {
    div()
        .h(px(NAV_ROW_H))
        .px(px(SPACE_2))
        .text_color(rgb(pal.text_disabled))
        .child(LOADING_ROW_TEXT)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_ranges_convert_without_overflow() {
        assert_eq!(visible(&(3..9)), 3..9);
        assert_eq!(index_u32(usize::MAX), u32::MAX);
    }
}
