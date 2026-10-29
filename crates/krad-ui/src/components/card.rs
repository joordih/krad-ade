use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{div, px, rgb, App, Div, IntoElement, ParentElement, Styled};

use crate::theme::{
    card_shadow, island_fill, surface_fill, window_material, Palette, RADIUS_LG, WINDOW_INSET,
};

pub(crate) fn card(pal: &Palette, cx: &App) -> Div {
    let translucent = window_material(cx).is_translucent();
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .rounded(px(RADIUS_LG))
        .bg(surface_fill(pal.bg_surface, cx))
        .when(translucent, |card| card.border_1().border_color(pal.border))
        .when(!translucent, |card| card.shadow(card_shadow(pal)))
}

pub(crate) fn solid_card(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .rounded(px(RADIUS_LG))
        .bg(rgb(pal.bg_surface))
        .shadow(card_shadow(pal))
}

pub(crate) fn split_island(
    side: impl IntoElement,
    main: impl IntoElement,
    pal: &Palette,
    cx: &App,
) -> Div {
    div()
        .flex()
        .flex_row()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .rounded(px(RADIUS_LG))
        .bg(island_fill(pal, cx))
        .when(window_material(cx).is_translucent(), |island| {
            island.border_1().border_color(pal.border)
        })
        .when(!window_material(cx).is_translucent(), |island| {
            island.shadow(card_shadow(pal))
        })
        .child(side)
        .child(
            card(pal, cx)
                .flex_1()
                .my(px(WINDOW_INSET))
                .mr(px(WINDOW_INSET))
                .child(main),
        )
}
