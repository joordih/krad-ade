use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, Stateful, Styled,
};

mod sheet;

pub(super) use sheet::*;

use crate::components::{icon, IconName};
use crate::theme::{
    row_fill, row_shadow, Palette, CTA_SM_H, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS,
    SPACE_1_5, TEXT_SM, TEXT_XS,
};

const SEGMENT_RADIUS: f32 = 7.0;
const SEGMENT_PAD_X: f32 = 10.0;

pub(super) fn field_label(text: &str, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_subtle))
        .child(text.to_owned())
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SegmentSize {
    Dialog,
    Compact,
}

pub(super) fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    glyph: Option<IconName>,
    selected: bool,
    size: SegmentSize,
    pal: &Palette,
    cx: &gpui_kit::App,
) -> Stateful<Div> {
    let dialog = size == SegmentSize::Dialog;
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .when(dialog, |item| {
            item.flex_1()
                .justify_center()
                .h(px(NAV_ROW_H))
                .rounded(px(SEGMENT_RADIUS))
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
        })
        .when(!dialog, |item| {
            item.flex_none()
                .h(px(CTA_SM_H))
                .px(px(SEGMENT_PAD_X))
                .rounded(px(RADIUS_XS))
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
        })
        .cursor_pointer()
        .text_color(rgb(if selected {
            pal.text_strong
        } else {
            pal.text_default
        }))
        .when(selected, |item| {
            item.bg(row_fill(pal, cx))
                .shadow(row_shadow(pal, cx))
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!selected && !dialog, |item| {
            item.hover(|style| style.bg(pal.bg_hover))
        })
        .children(glyph.map(|glyph| icon(glyph, ICON_SM, pal.text_default)))
        .child(label.into())
}
