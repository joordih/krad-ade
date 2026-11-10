use gpui_kit::{
    canvas, div, fill, point, px, rgb, size, App, Bounds, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, Rgba, Styled,
};
use krad_domain::{CanvasPoint, Pose, CANVAS_SIZE};
use std::rc::Rc;

use super::super::super::EnvironmentShell;
use super::KraddieStudio;
use crate::theme::{tinted, Palette};

pub(super) const CANVAS_PX: f32 = 432.0;
const CHECKER_ALPHA: u32 = 0x10;
const GRID_ALPHA: u32 = 0x18;
const GRID_LINE: f32 = 1.0;

pub(super) fn canvas_point(bounds: Bounds<Pixels>, at: Point<Pixels>) -> Option<CanvasPoint> {
    let cells = f32::from(CANVAS_SIZE);
    let side = bounds.size.width.as_f32().min(bounds.size.height.as_f32());
    if side <= 0.0 {
        return None;
    }
    let cell = side / cells;
    let x = (at.x.as_f32() - bounds.origin.x.as_f32()) / cell;
    let y = (at.y.as_f32() - bounds.origin.y.as_f32()) / cell;
    if !(0.0..cells).contains(&x) || !(0.0..cells).contains(&y) {
        return None;
    }
    CanvasPoint::new(cell_index(x)?, cell_index(y)?)
}

fn cell_index(position: f32) -> Option<u8> {
    (0..CANVAS_SIZE).find(|index| position < f32::from(index + 1))
}

#[derive(Clone, Copy)]
struct Inks {
    checker: Rgba,
    grid: Rgba,
}

fn paint_backdrop(bounds: Bounds<Pixels>, inks: Inks, window: &mut gpui_kit::Window) {
    let cells = f32::from(CANVAS_SIZE);
    let cell = bounds.size.width.as_f32() / cells;
    let (left, top) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
    for y in 0..CANVAS_SIZE {
        for x in (y % 2..CANVAS_SIZE).step_by(2) {
            let origin = point(
                px(left + f32::from(x) * cell),
                px(top + f32::from(y) * cell),
            );
            window.paint_quad(fill(
                Bounds::new(origin, size(px(cell), px(cell))),
                inks.checker,
            ));
        }
    }
    for line in 0..=CANVAS_SIZE {
        let offset = f32::from(line) * cell;
        let across = Bounds::new(
            point(px(left), px(top + offset)),
            size(bounds.size.width, px(GRID_LINE)),
        );
        let down = Bounds::new(
            point(px(left + offset), px(top)),
            size(px(GRID_LINE), bounds.size.height),
        );
        window.paint_quad(fill(across, inks.grid));
        window.paint_quad(fill(down, inks.grid));
    }
}

impl EnvironmentShell {
    pub(super) fn studio_canvas(
        studio: &KraddieStudio,
        pal: &Palette,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl IntoElement {
        let sprite = Rc::clone(&studio.sprite);
        let painted = Rc::clone(&studio.bounds);
        let inks = Inks {
            checker: tinted(pal.text_muted, CHECKER_ALPHA),
            grid: tinted(pal.text_muted, GRID_ALPHA),
        };
        div()
            .flex_none()
            .size(px(CANVAS_PX))
            .rounded(px(GRID_LINE))
            .bg(rgb(pal.bg_panel))
            .cursor_crosshair()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                    this.edit_studio(cx, |studio| {
                        let at = studio
                            .bounds
                            .get()
                            .and_then(|bounds| canvas_point(bounds, event.position));
                        if let Some(at) = at {
                            studio.press(at);
                        }
                    });
                }),
            )
            .on_mouse_move(
                cx.listener(|this: &mut Self, event: &MouseMoveEvent, _, cx| {
                    let drawing = this
                        .kraddies
                        .studio
                        .as_ref()
                        .is_some_and(|studio| studio.stroke.is_some());
                    if !drawing || event.pressed_button != Some(MouseButton::Left) {
                        return;
                    }
                    this.edit_studio(cx, |studio| {
                        let at = studio
                            .bounds
                            .get()
                            .and_then(|bounds| canvas_point(bounds, event.position));
                        if let Some(at) = at {
                            studio.drag(at);
                        }
                    });
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.edit_studio(cx, KraddieStudio::release);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.edit_studio(cx, KraddieStudio::release);
                }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _: &mut App| {
                        painted.set(Some(bounds));
                        paint_backdrop(bounds, inks, window);
                        sprite.paint(bounds, Pose::default(), window);
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_positions_map_onto_canvas_cells() {
        let bounds = Bounds::new(point(px(100.0), px(50.0)), size(px(240.0), px(240.0)));
        let at = |x: f32, y: f32| canvas_point(bounds, point(px(x), px(y)));
        assert_eq!(at(100.0, 50.0), CanvasPoint::new(0, 0));
        assert_eq!(at(339.9, 289.9), CanvasPoint::new(23, 23));
        assert_eq!(at(125.0, 75.0), CanvasPoint::new(2, 2));
        assert_eq!(at(99.0, 60.0), None);
        assert_eq!(at(341.0, 60.0), None);
    }
}
