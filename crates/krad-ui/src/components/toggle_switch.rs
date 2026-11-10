use gpui_kit::{
    div, px, rgb, AnimationExt as _, AnyElement, ElementId, IntoElement, ParentElement,
    SpringAnimation, Styled,
};

use super::motion::{MOTION_GLIDE, MOTION_HIDDEN, MOTION_SETTLED};
use crate::theme::{card_shadow, Palette};

const TRACK_W: f32 = 30.0;
const TRACK_H: f32 = 18.0;
const KNOB: f32 = 14.0;
const KNOB_INSET: f32 = (TRACK_H - KNOB) / 2.0;
const KNOB_TRAVEL: f32 = TRACK_W - KNOB - 2.0 * KNOB_INSET;

#[must_use]
fn knob_left(progress: f32) -> f32 {
    KNOB_INSET + KNOB_TRAVEL * progress.clamp(MOTION_HIDDEN, MOTION_SETTLED)
}

pub(crate) fn toggle_switch(id: impl Into<ElementId>, on: bool, pal: &Palette) -> AnyElement {
    let target = if on { MOTION_SETTLED } else { MOTION_HIDDEN };
    let (accent, knob, shadow) = (pal.accent, pal.bg_surface, card_shadow(pal));
    div()
        .relative()
        .flex_none()
        .w(px(TRACK_W))
        .h(px(TRACK_H))
        .rounded_full()
        .bg(rgb(pal.chip_bg))
        .with_spring(
            id,
            SpringAnimation::new(MOTION_GLIDE).to(target),
            move |track, progress| {
                let lit = progress.clamp(MOTION_HIDDEN, MOTION_SETTLED);
                track
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_full()
                            .bg(rgb(accent))
                            .opacity(lit),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(KNOB_INSET))
                            .left(px(knob_left(progress)))
                            .size(px(KNOB))
                            .rounded_full()
                            .bg(rgb(knob))
                            .shadow(shadow),
                    )
            },
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_knob_travels_inside_the_track() {
        assert!((knob_left(MOTION_HIDDEN) - KNOB_INSET).abs() < f32::EPSILON);
        assert!((knob_left(MOTION_SETTLED) + KNOB + KNOB_INSET - TRACK_W).abs() < f32::EPSILON);
        assert!((knob_left(2.0) - knob_left(MOTION_SETTLED)).abs() < f32::EPSILON);
    }
}
