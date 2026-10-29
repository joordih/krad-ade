use gpui_kit::{
    canvas, fill, point, px, size, AnyElement, App, Bounds, DispatchPhase, IntoElement,
    MouseMoveEvent, Pixels, Point, Rgba, Styled, Window,
};

use crate::theme::{tinted, Palette};

const PITCH: f32 = 22.0;
const DOT: f32 = 1.5;
const DOT_SWELL: f32 = 2.0;
const REACH: f32 = 150.0;
const REST_ALPHA: f32 = 0.16;
const LIT_ALPHA: f32 = 0.45;

fn pull(distance: f32) -> f32 {
    let near = (1.0 - distance / REACH).clamp(0.0, 1.0);
    near * near * (3.0 - 2.0 * near)
}

fn with_alpha(color: u32, alpha: f32) -> Rgba {
    let mut tint = tinted(color, 0);
    tint.a = alpha.clamp(0.0, 1.0);
    tint
}

const HOLE_MARGIN: f32 = PITCH / 2.0;

fn paint_field(
    bounds: Bounds<Pixels>,
    hole: Option<Bounds<Pixels>>,
    ink: u32,
    window: &mut Window,
) {
    let clear = hole.map(|hole| {
        Bounds::new(
            point(
                hole.origin.x - px(HOLE_MARGIN),
                hole.origin.y - px(HOLE_MARGIN),
            ),
            size(
                hole.size.width + px(2.0 * HOLE_MARGIN),
                hole.size.height + px(2.0 * HOLE_MARGIN),
            ),
        )
    });
    let pointer = window.mouse_position();
    let felt = Bounds::new(
        point(bounds.origin.x - px(REACH), bounds.origin.y - px(REACH)),
        size(
            bounds.size.width + px(2.0 * REACH),
            bounds.size.height + px(2.0 * REACH),
        ),
    );
    let near: Option<Point<Pixels>> = felt.contains(&pointer).then_some(pointer);
    let (left, top) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
    let (right, bottom) = (
        left + bounds.size.width.as_f32(),
        top + bounds.size.height.as_f32(),
    );
    let mut y = top + PITCH / 2.0;
    while y < bottom {
        let mut x = left + PITCH / 2.0;
        while x < right {
            if clear.is_some_and(|clear| clear.contains(&point(px(x), px(y)))) {
                x += PITCH;
                continue;
            }
            let strength = near.map_or(0.0, |pointer| {
                let (dx, dy) = (x - pointer.x.as_f32(), y - pointer.y.as_f32());
                pull(dx.hypot(dy))
            });
            let side = DOT + DOT_SWELL * strength;
            let alpha = REST_ALPHA + (LIT_ALPHA - REST_ALPHA) * strength;
            let square = Bounds::new(
                point(px(x - side / 2.0), px(y - side / 2.0)),
                size(px(side), px(side)),
            );
            window.paint_quad(fill(square, with_alpha(ink, alpha)).corner_radii(px(side / 2.0)));
            x += PITCH;
        }
        y += PITCH;
    }
    let glowing = near.is_some();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _| {
        if phase == DispatchPhase::Bubble && (glowing || felt.contains(&event.position)) {
            window.refresh();
        }
    });
}

pub(super) fn dot_field(pal: &Palette, hole: Option<Bounds<Pixels>>) -> AnyElement {
    let ink = pal.text_muted;
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _: &mut App| paint_field(bounds, hole, ink, window),
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pull_fades_from_the_pointer_to_the_reach() {
        assert!((pull(0.0) - 1.0).abs() < f32::EPSILON);
        assert!(pull(REACH / 2.0) > 0.0 && pull(REACH / 2.0) < 1.0);
        assert!(pull(REACH).abs() < f32::EPSILON && pull(REACH * 2.0).abs() < f32::EPSILON);
    }
}
