use gpui_kit::{div, px, rgb, ClickEvent, Div, FontWeight, ParentElement, Styled};

use crate::theme::{Palette, LINE_XS, RADIUS_LG, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_XS};

pub(super) fn field(label: &'static str, pal: &Palette) -> Div {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(pal.text_default))
        .child(label)
}

pub(super) fn hint(text: &'static str, pal: &Palette) -> Div {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_subtle))
        .child(text)
}

pub(super) fn panel(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_2))
        .p(px(SPACE_3))
        .rounded(px(RADIUS_LG))
        .border_1()
        .border_color(pal.divider)
        .bg(rgb(pal.bg_panel))
}

pub(super) fn chips() -> Div {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(SPACE_1))
}

pub(super) fn field_group() -> Div {
    div().flex().flex_col().gap(px(SPACE_1_5))
}

pub(super) fn click_origin(event: &ClickEvent) -> (f32, f32) {
    let at = event.position();
    (at.x.as_f32(), at.y.as_f32())
}
