use gpui_kit::{
    canvas, div, fill, point, px, rgb, size, App, Bounds, IntoElement, ParentElement, Pixels, Rgba,
    Styled, Window,
};
use krad_domain::{CanvasPoint, KraddieAnimation, KraddieDesign, KraddieMood, Pose, CANVAS_SIZE};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Run {
    x: u8,
    y: u8,
    len: u8,
    color: Rgba,
}

#[derive(Debug, PartialEq)]
pub(crate) struct KraddieSprite {
    runs: Vec<Run>,
    pub(crate) animation: KraddieAnimation,
}

impl KraddieSprite {
    pub(crate) fn new(design: &KraddieDesign, mood: KraddieMood) -> Self {
        let mut runs: Vec<Run> = Vec::new();
        for y in 0..CANVAS_SIZE {
            for x in 0..CANVAS_SIZE {
                let Some(color) = CanvasPoint::new(x, y).and_then(|at| design.pixel(at, mood))
                else {
                    continue;
                };
                let color = rgb(color.hex());
                match runs.last_mut() {
                    Some(run) if run.y == y && run.x + run.len == x && run.color == color => {
                        run.len += 1;
                    }
                    _ => runs.push(Run {
                        x,
                        y,
                        len: 1,
                        color,
                    }),
                }
            }
        }
        Self {
            runs,
            animation: design.moods.get(mood).animation,
        }
    }

    pub(crate) fn moves(&self) -> bool {
        self.animation.moves()
    }

    pub(crate) fn paint(&self, bounds: Bounds<Pixels>, pose: Pose, window: &mut Window) {
        let cells = f32::from(CANVAS_SIZE);
        let side = bounds.size.width.as_f32().min(bounds.size.height.as_f32());
        let cell = side / cells;
        let Pose { dx, dy, squash } = pose;
        let squash = f32::from(squash);
        let row_h = cell * (cells - squash) / cells;
        let left = bounds.origin.x.as_f32() + (bounds.size.width.as_f32() - side) / 2.0;
        let top = bounds.origin.y.as_f32() + (bounds.size.height.as_f32() - side) / 2.0;
        let left = left + f32::from(dx) * cell;
        let top = top + f32::from(dy) * cell + squash * cell;
        for run in &self.runs {
            let origin = point(
                px(left + f32::from(run.x) * cell),
                px(top + f32::from(run.y) * row_h),
            );
            let extent = size(px(f32::from(run.len) * cell), px(row_h));
            window.paint_quad(fill(Bounds::new(origin, extent), run.color));
        }
    }
}

pub(crate) fn kraddie_avatar(
    sprite: &Rc<KraddieSprite>,
    side: f32,
    frame: u64,
) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(side))
        .child(kraddie_canvas(sprite, frame))
}

pub(crate) fn kraddie_canvas(sprite: &Rc<KraddieSprite>, frame: u64) -> impl IntoElement {
    let sprite = Rc::clone(sprite);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _: &mut App| {
            sprite.paint(bounds, sprite.animation.pose(frame), window);
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::BodyStyle;

    #[test]
    fn a_sprite_merges_each_rows_same_colored_pixels() {
        let design = KraddieDesign::kraddie(BodyStyle::default());
        let sprite = KraddieSprite::new(&design, KraddieMood::Idle);
        let pixels: usize = sprite.runs.iter().map(|run| usize::from(run.len)).sum();
        let painted = (0..CANVAS_SIZE)
            .flat_map(|y| (0..CANVAS_SIZE).map(move |x| (x, y)))
            .filter_map(|(x, y)| CanvasPoint::new(x, y))
            .filter(|at| design.pixel(*at, KraddieMood::Idle).is_some())
            .count();
        assert_eq!(pixels, painted);
        assert!(sprite.runs.len() <= painted);
        assert_eq!(sprite.animation, design.moods.idle.animation);
        let working = KraddieSprite::new(&design, KraddieMood::Working);
        assert_ne!(working, sprite);
    }
}
