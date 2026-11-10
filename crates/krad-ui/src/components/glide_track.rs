use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, relative, rgb, AnimationExt as _, AnyElement, App, ElementId, FontWeight,
    InteractiveElement, IntoElement, ParentElement, SharedString, SpringAnimation,
    StatefulInteractiveElement, Styled, Window,
};
use std::rc::Rc;

use super::icon::{icon, IconName};
use super::motion::MOTION_GLIDE;
use crate::theme::{
    row_fill, row_shadow, Palette, CAPSULE_PAD, CTA_SM_H, ICON_SM, LINE_XS, SPACE_1, TEXT_XS,
};

const PRESSED_OPACITY: f32 = 0.7;

pub(crate) type GlidePick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GlideSizing {
    Fill,
    Scroll(f32),
}

#[must_use]
pub(crate) fn track_min_width(sizing: GlideSizing, count: usize) -> f32 {
    match sizing {
        GlideSizing::Fill => 0.0,
        GlideSizing::Scroll(option) => {
            f32::from(u16::try_from(count).unwrap_or(u16::MAX)) * option + 2.0 * CAPSULE_PAD
        }
    }
}

pub(crate) struct GlideOption {
    pub(crate) label: SharedString,
    pub(crate) glyph: Option<IconName>,
}

#[must_use]
pub(crate) fn indicator_span(selected: usize, count: usize) -> (f32, f32) {
    let parts = f32::from(u16::try_from(count.max(1)).unwrap_or(u16::MAX));
    let at = f32::from(u16::try_from(selected).unwrap_or(u16::MAX));
    (at / parts, 1.0 / parts)
}

pub(crate) fn glide_track(
    id: &'static str,
    options: Vec<GlideOption>,
    selected: Option<usize>,
    on_pick: &GlidePick,
    sizing: GlideSizing,
    pal: &Palette,
    cx: &App,
) -> AnyElement {
    let count = options.len();
    let min_w = track_min_width(sizing, count);
    let fill = row_fill(pal, cx);
    let shadow = row_shadow(pal, cx);
    let divider = pal.divider;
    let buttons = options.into_iter().enumerate().map(|(index, option)| {
        let chosen = selected == Some(index);
        let pick = Rc::clone(on_pick);
        div()
            .id(ElementId::NamedInteger(
                id.into(),
                u64::try_from(index).unwrap_or_default(),
            ))
            .flex()
            .flex_row()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .overflow_hidden()
            .items_center()
            .justify_center()
            .gap(px(SPACE_1))
            .h(px(CTA_SM_H))
            .px(px(SPACE_1))
            .rounded_full()
            .cursor_pointer()
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .whitespace_nowrap()
            .text_color(rgb(if chosen {
                pal.text_strong
            } else {
                pal.text_muted
            }))
            .when(chosen, |button| button.font_weight(FontWeight::MEDIUM))
            .when(!chosen, |button| {
                button.hover(|style| style.text_color(rgb(pal.text_default)))
            })
            .active(|style| style.opacity(PRESSED_OPACITY))
            .on_click(move |_, window, cx| pick(index, window, cx))
            .children(option.glyph.map(|glyph| {
                icon(
                    glyph,
                    ICON_SM,
                    if chosen {
                        pal.text_strong
                    } else {
                        pal.text_muted
                    },
                )
            }))
            .child(div().min_w_0().truncate().child(option.label))
    });
    let indicator = selected.map(|selected| {
        let (left, width) = indicator_span(selected, count);
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(relative(width))
            .rounded_full()
            .bg(fill)
            .border_1()
            .border_color(divider)
            .shadow(shadow)
            .with_spring(
                ElementId::from(id),
                SpringAnimation::new(MOTION_GLIDE).to(left),
                |pill, left| pill.left(relative(left)),
            )
    });
    div()
        .flex()
        .w_full()
        .min_w(px(min_w))
        .p(px(CAPSULE_PAD))
        .rounded_full()
        .bg(pal.bg_hover)
        .child(
            div()
                .relative()
                .flex()
                .flex_row()
                .flex_1()
                .min_w_0()
                .children(indicator)
                .children(buttons),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scrolling_track_keeps_room_for_every_option() {
        assert!(track_min_width(GlideSizing::Fill, 6).abs() < f32::EPSILON);
        assert!((track_min_width(GlideSizing::Scroll(100.0), 6) - 604.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_indicator_covers_exactly_the_chosen_slot() {
        assert_eq!(indicator_span(0, 4), (0.0, 0.25));
        assert_eq!(indicator_span(3, 4), (0.75, 0.25));
        assert_eq!(indicator_span(0, 0), (0.0, 1.0));
    }
}
