use gpui_kit::{px, Animation, AnimationExt as _, AnyElement, ElementId, IntoElement, Styled};
use std::time::Duration;

use super::placement::{Edge, Placed};
use crate::components::motion::{ease_out_cubic, MOTION_SETTLED};
use crate::motion::ease_out_emphasized;

pub(crate) const FLOATING_OPEN: Duration = Duration::from_millis(170);
pub(crate) const FLOATING_SHIFT: f32 = 8.0;
pub(crate) const DIALOG_ENTER: Duration = Duration::from_millis(200);
pub(crate) const DIALOG_RISE: f32 = 6.0;

#[must_use]
pub(crate) fn approach_offset(placed: &Placed, shown: f32) -> (f32, f32) {
    let (dx, dy) = placed.side.toward_anchor();
    let still = FLOATING_SHIFT * (MOTION_SETTLED - shown);
    (dx * still, dy * still)
}

pub(crate) fn floating_at<E>(id: impl Into<ElementId>, placed: Placed, surface: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    surface
        .with_animation(
            id,
            Animation::new(FLOATING_OPEN).with_easing(ease_out_emphasized),
            move |surface, shown| {
                let (dx, dy) = approach_offset(&placed, shown);
                let surface = surface.opacity(shown).left(px(placed.left + dx));
                match placed.edge {
                    Edge::Top(top) => surface.top(px(top + dy)),
                    Edge::Bottom(bottom) => surface.bottom(px(bottom - dy)),
                }
            },
        )
        .into_any_element()
}

pub(crate) fn dialog_enter<E>(id: impl Into<ElementId>, dialog: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    dialog
        .with_animation(
            id,
            Animation::new(DIALOG_ENTER).with_easing(ease_out_cubic),
            |dialog, shown| {
                dialog
                    .opacity(shown)
                    .top(px(DIALOG_RISE * (MOTION_SETTLED - shown)))
            },
        )
        .into_any_element()
}

pub(crate) fn backdrop_fade<E>(id: impl Into<ElementId>, backdrop: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    backdrop
        .with_animation(
            id,
            Animation::new(DIALOG_ENTER).with_easing(ease_out_cubic),
            Styled::opacity,
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::super::placement::Side;
    use super::*;

    #[test]
    fn a_surface_slides_in_from_its_anchor_and_settles_in_place() {
        let placed = Placed {
            left: 10.0,
            edge: Edge::Top(20.0),
            max_height: 100.0,
            side: Side::Below,
        };
        assert_eq!(approach_offset(&placed, 0.0), (0.0, -FLOATING_SHIFT));
        assert_eq!(approach_offset(&placed, 1.0), (0.0, 0.0));
        let submenu = Placed {
            side: Side::Before,
            ..placed
        };
        assert_eq!(approach_offset(&submenu, 0.0), (FLOATING_SHIFT, 0.0));
    }
}
