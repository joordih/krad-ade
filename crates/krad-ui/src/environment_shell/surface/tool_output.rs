use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, rgb, AnyElement, IntoElement, ParentElement, SharedString, Styled};

use super::super::chat::ShownOutput;
use crate::theme::{Palette, LINE_XS, RADIUS_SM, SPACE_1, SPACE_2, TEXT_XS};

pub(super) fn tool_output(output: &ShownOutput, mono: &SharedString, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .mt(px(SPACE_1))
        .py(px(SPACE_1))
        .px(px(SPACE_2))
        .rounded(px(RADIUS_SM))
        .overflow_hidden()
        .bg(rgb(pal.bg_surface))
        .font_family(mono.clone())
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_default))
        .when(!output.text.is_empty(), |block| {
            block.child(output.text.clone())
        })
        .children(
            output
                .failure
                .clone()
                .map(|failure| div().text_color(rgb(pal.error_fg)).child(failure)),
        )
        .into_any_element()
}
