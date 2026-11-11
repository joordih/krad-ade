use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, rgb, AnyElement, App, IntoElement, ParentElement, SharedString, Styled};

use super::super::chat::ShownOutput;
use crate::theme::{
    level_fill, Palette, SurfaceLevel, LINE_XS, RADIUS_SM, SPACE_1, SPACE_2, TEXT_XS,
};

pub(super) fn tool_output(
    output: &ShownOutput,
    mono: &SharedString,
    pal: &Palette,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .mt(px(SPACE_1))
        .py(px(SPACE_1))
        .px(px(SPACE_2))
        .rounded(px(RADIUS_SM))
        .overflow_hidden()
        .bg(level_fill(SurfaceLevel::Pane, pal.bg_surface, cx))
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
