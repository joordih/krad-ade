mod composer;
mod suggestions;

use gpui_kit::base::SelectableText;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, relative, rgb, AnyElement, ElementId, IntoElement, ParentElement, Styled};

use crate::theme::{Palette, RADIUS_LG, SPACE_0_5, SPACE_3, TEXT_SM, TEXT_XS};

const BUBBLE_MAX_W: f32 = 0.82;
const BUBBLE_PAD_X: f32 = SPACE_3;
const BUBBLE_PAD_Y: f32 = 10.0;
const BUBBLE_LINE_H: f32 = 19.0;

const QUEUED_LABEL: &str = "Queued";

pub(in crate::environment_shell) fn question_bubble(
    id: ElementId,
    text: &str,
    queued: bool,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .items_end()
        .gap(px(SPACE_0_5))
        .child(
            div()
                .max_w(relative(BUBBLE_MAX_W))
                .px(px(BUBBLE_PAD_X))
                .py(px(BUBBLE_PAD_Y))
                .rounded(px(RADIUS_LG))
                .bg(rgb(pal.bg_panel))
                .text_size(px(TEXT_SM))
                .line_height(px(BUBBLE_LINE_H))
                .text_color(rgb(pal.text_strong))
                .child(SelectableText::new(id, text.to_owned())),
        )
        .when(queued, |bubble| {
            bubble.child(
                div()
                    .text_size(px(TEXT_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(QUEUED_LABEL),
            )
        })
        .into_any_element()
}
