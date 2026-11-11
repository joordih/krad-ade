use gpui_kit::{point, px, size, Bounds, Pixels, Point, Size};

use crate::theme::{FLOATING_EDGE_MARGIN, FLOATING_GAP};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Below,
    Above,
    After,
    Before,
}

impl Side {
    const fn flipped(self) -> Self {
        match self {
            Self::Below => Self::Above,
            Self::Above => Self::Below,
            Self::After => Self::Before,
            Self::Before => Self::After,
        }
    }

    pub(crate) const fn toward_anchor(self) -> (f32, f32) {
        match self {
            Self::Below => (0.0, -1.0),
            Self::Above => (0.0, 1.0),
            Self::After => (-1.0, 0.0),
            Self::Before => (1.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Anchor {
    Point(Point<Pixels>),
    Bounds(Bounds<Pixels>),
}

impl Anchor {
    fn bounds(self) -> Bounds<Pixels> {
        match self {
            Self::Point(at) => Bounds::new(at, size(Pixels::ZERO, Pixels::ZERO)),
            Self::Bounds(bounds) => bounds,
        }
    }

    const fn gap(self) -> f32 {
        match self {
            Self::Point(_) => 0.0,
            Self::Bounds(_) => FLOATING_GAP,
        }
    }

    #[must_use]
    pub(crate) fn relative_to(self, (x, y): (f32, f32)) -> Self {
        let shift = point(px(-x), px(-y));
        match self {
            Self::Point(at) => Self::Point(at + shift),
            Self::Bounds(bounds) => Self::Bounds(bounds + shift),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Edge {
    Top(f32),
    Bottom(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    pub(crate) left: f32,
    pub(crate) edge: Edge,
    pub(crate) max_height: f32,
    pub(crate) side: Side,
}

impl Placed {
    #[must_use]
    pub(crate) fn top(&self, viewport_height: f32, height: f32) -> f32 {
        match self.edge {
            Edge::Top(top) => top,
            Edge::Bottom(bottom) => viewport_height - bottom - height.min(self.max_height),
        }
    }
}

struct Span {
    start: f32,
    end: f32,
}

fn shifted(start: f32, length: f32, viewport: f32) -> f32 {
    start
        .min(viewport - length - FLOATING_EDGE_MARGIN)
        .max(FLOATING_EDGE_MARGIN)
}

fn room(side: Side, along: &Span, viewport: f32, gap: f32) -> f32 {
    match side {
        Side::Below | Side::After => viewport - FLOATING_EDGE_MARGIN - (along.end + gap),
        Side::Above | Side::Before => along.start - gap - FLOATING_EDGE_MARGIN,
    }
}

fn chosen_side(preferred: Side, along: &Span, length: f32, viewport: f32, gap: f32) -> Side {
    let other = preferred.flipped();
    let wanted_room = room(preferred, along, viewport, gap);
    if wanted_room >= length {
        return preferred;
    }
    let other_room = room(other, along, viewport, gap);
    if other_room >= length || other_room > wanted_room {
        other
    } else {
        preferred
    }
}

#[must_use]
pub(crate) fn place(
    anchor: Anchor,
    wanted: Size<Pixels>,
    viewport: Size<Pixels>,
    preferred: Side,
) -> Placed {
    let bounds = anchor.bounds();
    let gap = anchor.gap();
    let (width, height) = (wanted.width.as_f32(), wanted.height.as_f32());
    let (view_w, view_h) = (viewport.width.as_f32(), viewport.height.as_f32());
    let horizontal = Span {
        start: bounds.left().as_f32(),
        end: bounds.right().as_f32(),
    };
    let vertical = Span {
        start: bounds.top().as_f32(),
        end: bounds.bottom().as_f32(),
    };
    let tallest = (view_h - 2.0 * FLOATING_EDGE_MARGIN).max(0.0);
    match preferred {
        Side::Below | Side::Above => {
            let side = chosen_side(preferred, &vertical, height, view_h, gap);
            let max_height = room(side, &vertical, view_h, gap).min(height).max(0.0);
            let edge = match side {
                Side::Above => Edge::Bottom(view_h - (vertical.start - gap)),
                _ => Edge::Top(vertical.end + gap),
            };
            Placed {
                left: shifted(horizontal.start, width, view_w),
                edge,
                max_height,
                side,
            }
        }
        Side::After | Side::Before => {
            let side = chosen_side(preferred, &horizontal, width, view_w, gap);
            let left = match side {
                Side::Before => horizontal.start - gap - width,
                _ => horizontal.end + gap,
            };
            let max_height = height.min(tallest);
            Placed {
                left: shifted(left, width, view_w),
                edge: Edge::Top(shifted(vertical.start, max_height, view_h)),
                max_height,
                side,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: (f32, f32) = (1000.0, 800.0);

    fn viewport() -> Size<Pixels> {
        size(px(VIEWPORT.0), px(VIEWPORT.1))
    }

    fn wanted(width: f32, height: f32) -> Size<Pixels> {
        size(px(width), px(height))
    }

    fn button(left: f32, top: f32) -> Anchor {
        Anchor::Bounds(Bounds::new(
            point(px(left), px(top)),
            size(px(40.0), px(20.0)),
        ))
    }

    #[test]
    fn a_surface_opens_below_its_anchor_when_it_fits() {
        let placed = place(
            button(100.0, 100.0),
            wanted(200.0, 300.0),
            viewport(),
            Side::Below,
        );
        assert_eq!(placed.side, Side::Below);
        assert_eq!(placed.edge, Edge::Top(120.0 + FLOATING_GAP));
        assert!((placed.left - 100.0).abs() < f32::EPSILON);
        assert!((placed.max_height - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_surface_flips_above_when_there_is_no_room_below() {
        let placed = place(
            button(100.0, 700.0),
            wanted(200.0, 300.0),
            viewport(),
            Side::Below,
        );
        assert_eq!(placed.side, Side::Above);
        assert_eq!(placed.edge, Edge::Bottom(VIEWPORT.1 - 700.0 + FLOATING_GAP));
        let top = placed.top(VIEWPORT.1, 300.0);
        assert!((top + 300.0 - (700.0 - FLOATING_GAP)).abs() < f32::EPSILON);
    }

    #[test]
    fn a_surface_that_fits_nowhere_takes_the_roomier_side_and_scrolls() {
        let placed = place(
            button(100.0, 500.0),
            wanted(200.0, 900.0),
            viewport(),
            Side::Below,
        );
        assert_eq!(placed.side, Side::Above);
        let room_above = 500.0 - FLOATING_GAP - FLOATING_EDGE_MARGIN;
        assert!((placed.max_height - room_above).abs() < f32::EPSILON);
    }

    #[test]
    fn a_surface_shifts_back_inside_the_window_edges() {
        let right = place(
            button(950.0, 100.0),
            wanted(200.0, 100.0),
            viewport(),
            Side::Below,
        );
        assert!((right.left - (VIEWPORT.0 - 200.0 - FLOATING_EDGE_MARGIN)).abs() < f32::EPSILON);
        let left = place(
            button(-30.0, 100.0),
            wanted(200.0, 100.0),
            viewport(),
            Side::Below,
        );
        assert!((left.left - FLOATING_EDGE_MARGIN).abs() < f32::EPSILON);
    }

    #[test]
    fn a_submenu_opens_after_its_row_and_flips_before_it_at_the_right_edge() {
        let row = |left: f32| {
            Anchor::Bounds(Bounds::new(
                point(px(left), px(200.0)),
                size(px(236.0), px(30.0)),
            ))
        };
        let after = place(row(100.0), wanted(300.0, 200.0), viewport(), Side::After);
        assert_eq!(after.side, Side::After);
        assert!((after.left - (336.0 + FLOATING_GAP)).abs() < f32::EPSILON);
        assert_eq!(after.edge, Edge::Top(200.0));
        let before = place(row(700.0), wanted(300.0, 200.0), viewport(), Side::After);
        assert_eq!(before.side, Side::Before);
        assert!((before.left - (700.0 - FLOATING_GAP - 300.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn a_submenu_near_the_bottom_shifts_up_to_stay_visible() {
        let row = Anchor::Bounds(Bounds::new(
            point(px(100.0), px(760.0)),
            size(px(236.0), px(30.0)),
        ));
        let placed = place(row, wanted(300.0, 200.0), viewport(), Side::After);
        assert_eq!(
            placed.edge,
            Edge::Top(VIEWPORT.1 - 200.0 - FLOATING_EDGE_MARGIN)
        );
    }

    #[test]
    fn a_cursor_anchor_opens_flush_with_the_pointer() {
        let placed = place(
            Anchor::Point(point(px(300.0), px(300.0))),
            wanted(200.0, 100.0),
            viewport(),
            Side::Below,
        );
        assert_eq!(placed.edge, Edge::Top(300.0));
        assert!((placed.left - 300.0).abs() < f32::EPSILON);
    }
}
