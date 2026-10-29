use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, rgb, Div, ParentElement, Styled};

use super::icon::{icon, IconName};
use crate::theme::{Palette, RADIUS_XS};

const BOX: f32 = 14.0;
const TICK: f32 = 12.0;
const BORDER: f32 = 1.0;

pub(crate) fn check_box(checked: bool, pal: &Palette) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(BOX))
        .rounded(px(RADIUS_XS / 2.0))
        .when(checked, |check| {
            check
                .bg(rgb(pal.accent))
                .child(icon(IconName::Tick, TICK, pal.on_accent))
        })
        .when(!checked, |check| {
            check
                .border(px(BORDER))
                .border_color(rgb(pal.text_muted))
                .bg(rgb(pal.bg_surface))
        })
}
