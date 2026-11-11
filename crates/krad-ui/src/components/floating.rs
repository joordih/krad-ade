mod motion;
mod placement;

use gpui_kit::{div, px, rgb, App, Div, InteractiveElement, Styled};

pub(crate) use motion::{backdrop_fade, dialog_enter, floating_at};
pub(crate) use placement::{place, Anchor, Edge, Placed, Side};

use crate::theme::{
    dialog_scrim, floating_border, floating_shadows, popover_fill, Palette, DIALOG_RADIUS,
    FLOATING_PAD, FLOATING_RADIUS,
};

pub(crate) fn floating_frame(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(FLOATING_RADIUS))
        .border_1()
        .border_color(floating_border(pal))
        .shadow(floating_shadows())
        .overflow_hidden()
}

pub(crate) fn floating_surface(pal: &Palette, cx: &App) -> Div {
    floating_frame(pal).bg(popover_fill(pal, cx))
}

pub(crate) fn floating_panel(pal: &Palette, cx: &App) -> Div {
    floating_surface(pal, cx)
        .absolute()
        .p(px(FLOATING_PAD))
        .occlude()
}

pub(crate) fn dialog_surface(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(DIALOG_RADIUS))
        .bg(rgb(pal.bg_surface))
        .border_1()
        .border_color(floating_border(pal))
        .shadow(floating_shadows())
        .overflow_hidden()
}

pub(crate) fn dialog_backdrop(pal: &Palette, cx: &App) -> Div {
    div().absolute().inset_0().bg(dialog_scrim(pal, cx))
}
