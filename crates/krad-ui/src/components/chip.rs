use gpui_kit::{div, px, rgb, Div, FontWeight, ParentElement, SharedString, Styled};

use crate::theme::{Palette, CHIP_H, LINE_2XS, SPACE_2, TEXT_2XS};

pub(crate) fn chip(text: impl Into<SharedString>, pal: &Palette) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(CHIP_H))
        .px(px(SPACE_2))
        .rounded_full()
        .bg(rgb(pal.chip_bg))
        .text_color(rgb(pal.chip_fg))
        .text_size(px(TEXT_2XS))
        .line_height(px(LINE_2XS))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(text.into())
}
