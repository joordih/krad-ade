use krad_domain::{StackDirection, ToastPlacement, WindowSide};

use crate::theme::{SPACE_2, SPACE_3, TOOLBAR_H, WINDOW_INSET};

pub(super) const GRID: f32 = 8.0;
const SNAP_DISTANCE: f32 = 6.0;
const EDGE: f32 = WINDOW_INSET + SPACE_3;
const TOP: f32 = WINDOW_INSET + TOOLBAR_H + WINDOW_INSET + SPACE_2;
const HALF: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Anchor {
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Guides {
    pub(super) x: Option<f32>,
    pub(super) y: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Snapping {
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Frame {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) sidebar: f32,
}

impl Frame {
    pub(super) fn anchor_of(self, placement: Option<ToastPlacement>) -> Anchor {
        let anchor = placement.map_or(
            Anchor {
                x: self.width - EDGE,
                y: TOP,
            },
            |placement| Anchor {
                x: placement.x.value() * self.width,
                y: placement.y.value() * self.height,
            },
        );
        self.clamp(anchor)
    }

    pub(super) fn placement_of(self, anchor: Anchor) -> ToastPlacement {
        ToastPlacement::new(
            anchor.x / self.width.max(1.0),
            anchor.y / self.height.max(1.0),
        )
    }

    pub(super) fn side_of(self, anchor: Anchor) -> WindowSide {
        self.placement_of(anchor).side()
    }

    pub(super) fn direction_of(self, anchor: Anchor) -> StackDirection {
        self.placement_of(anchor).direction()
    }

    pub(super) fn snap(self, anchor: Anchor, snapping: Snapping) -> (Anchor, Guides) {
        let anchor = self.clamp(anchor);
        if snapping == Snapping::Off {
            return (anchor, Guides::default());
        }
        let x_targets = [
            EDGE,
            self.sidebar + EDGE,
            self.width * HALF,
            self.width - EDGE,
        ];
        let y_targets = [TOP, self.height * HALF, self.height - EDGE];
        let (x, guide_x) = snap_axis(anchor.x, &x_targets);
        let (y, guide_y) = snap_axis(anchor.y, &y_targets);
        (
            self.clamp(Anchor { x, y }),
            Guides {
                x: guide_x,
                y: guide_y,
            },
        )
    }

    fn clamp(self, anchor: Anchor) -> Anchor {
        Anchor {
            x: anchor.x.clamp(EDGE, (self.width - EDGE).max(EDGE)),
            y: anchor.y.clamp(EDGE, (self.height - EDGE).max(EDGE)),
        }
    }
}

fn snap_axis(value: f32, targets: &[f32]) -> (f32, Option<f32>) {
    let nearest = targets
        .iter()
        .copied()
        .filter(|target| (target - value).abs() <= SNAP_DISTANCE)
        .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()));
    match nearest {
        Some(target) => (target, Some(target)),
        None => ((value / GRID).round() * GRID, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Frame = Frame {
        width: 1200.0,
        height: 800.0,
        sidebar: 248.0,
    };

    #[test]
    fn without_a_placement_toasts_sit_top_right_under_the_top_bar() {
        let anchor = FRAME.anchor_of(None);
        assert_eq!(
            anchor,
            Anchor {
                x: 1200.0 - EDGE,
                y: TOP
            }
        );
        assert_eq!(FRAME.side_of(anchor), WindowSide::Right);
        assert_eq!(FRAME.direction_of(anchor), StackDirection::Down);
    }

    #[test]
    fn a_point_near_a_line_snaps_to_it_and_shows_its_guide() {
        let (anchor, guides) = FRAME.snap(Anchor { x: 603.0, y: 397.0 }, Snapping::On);
        assert_eq!(anchor, Anchor { x: 600.0, y: 400.0 });
        assert_eq!(
            guides,
            Guides {
                x: Some(600.0),
                y: Some(400.0)
            }
        );
        let (beside_sidebar, guides) = FRAME.snap(Anchor { x: 262.0, y: 101.0 }, Snapping::On);
        assert!((beside_sidebar.x - (248.0 + EDGE)).abs() < f32::EPSILON);
        assert!(guides.x.is_some());
    }

    #[test]
    fn a_point_away_from_every_line_snaps_to_the_grid() {
        let (anchor, guides) = FRAME.snap(Anchor { x: 413.0, y: 237.0 }, Snapping::On);
        assert_eq!(anchor, Anchor { x: 416.0, y: 240.0 });
        assert_eq!(guides, Guides::default());
    }

    #[test]
    fn with_snapping_off_the_point_stays_where_it_was_dropped() {
        let dropped = Anchor { x: 413.0, y: 237.0 };
        assert_eq!(
            FRAME.snap(dropped, Snapping::Off),
            (dropped, Guides::default())
        );
    }

    #[test]
    fn a_point_outside_the_window_is_pulled_back_in() {
        let (anchor, _) = FRAME.snap(
            Anchor {
                x: -50.0,
                y: 5000.0,
            },
            Snapping::Off,
        );
        assert_eq!(
            anchor,
            Anchor {
                x: EDGE,
                y: 800.0 - EDGE
            }
        );
    }

    #[test]
    fn the_centre_line_flips_the_side_and_the_placement_survives_a_resize() {
        let left = Anchor { x: 599.0, y: 700.0 };
        assert_eq!(FRAME.side_of(left), WindowSide::Left);
        assert_eq!(FRAME.direction_of(left), StackDirection::Up);
        let placement = FRAME.placement_of(Anchor { x: 300.0, y: 200.0 });
        let wider = Frame {
            width: 2400.0,
            ..FRAME
        };
        let moved = wider.anchor_of(Some(placement));
        assert!((moved.x - 600.0).abs() < 0.5);
        assert!((moved.y - 200.0).abs() < 0.5);
    }
}
