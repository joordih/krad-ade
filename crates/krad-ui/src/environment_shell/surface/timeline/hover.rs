use gpui_kit::{Bounds, Pixels, Point};
use std::cell::Cell;
use std::rc::Rc;

pub(super) type Area = Rc<Cell<Option<Bounds<Pixels>>>>;

#[derive(Default)]
pub(super) struct Hover {
    pub(super) open: bool,
    apex: Option<Point<Pixels>>,
    pub(super) strip: Area,
    pub(super) card: Area,
}

fn cross(from: Point<Pixels>, to: Point<Pixels>, point: Point<Pixels>) -> f32 {
    (to.x - from.x).as_f32() * (point.y - from.y).as_f32()
        - (to.y - from.y).as_f32() * (point.x - from.x).as_f32()
}

fn in_triangle(
    apex: Point<Pixels>,
    top: Point<Pixels>,
    bottom: Point<Pixels>,
    point: Point<Pixels>,
) -> bool {
    let sides = [
        cross(apex, top, point),
        cross(top, bottom, point),
        cross(bottom, apex, point),
    ];
    sides.iter().all(|side| *side >= 0.0) || sides.iter().all(|side| *side <= 0.0)
}

impl Hover {
    pub(super) fn track(&mut self, point: Point<Pixels>) -> bool {
        let was_open = self.open;
        let over_strip = self.strip.get().is_some_and(|strip| strip.contains(&point));
        let card = self.card.get();
        if over_strip {
            self.open = true;
            self.apex = Some(point);
        } else if !self.open {
            return false;
        } else if let Some(card) = card {
            if card.contains(&point) {
                self.apex = None;
            } else if !self
                .apex
                .is_some_and(|apex| in_triangle(apex, card.top_right(), card.bottom_right(), point))
            {
                self.close();
            }
        }
        self.open != was_open
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.apex = None;
        self.card.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    fn at(x: f32, y: f32) -> Point<Pixels> {
        point(px(x), px(y))
    }

    fn hover() -> Hover {
        let hover = Hover::default();
        hover.strip.set(Some(Bounds::new(
            at(400.0, 180.0),
            size(px(20.0), px(40.0)),
        )));
        hover.card.set(Some(Bounds::new(
            at(90.0, 100.0),
            size(px(300.0), px(200.0)),
        )));
        hover
    }

    #[test]
    fn hovering_the_ticks_opens_the_list() {
        let mut hover = hover();
        assert!(hover.track(at(410.0, 200.0)));
        assert!(hover.open);
    }

    #[test]
    fn crossing_the_gap_towards_a_row_keeps_the_list_open() {
        let mut hover = hover();
        hover.track(at(410.0, 200.0));
        hover.track(at(398.0, 200.0));
        assert!(!hover.track(at(394.0, 150.0)));
        assert!(hover.open);
        hover.track(at(380.0, 120.0));
        assert!(hover.open);
    }

    #[test]
    fn leaving_away_from_the_list_closes_it() {
        let mut hover = hover();
        hover.track(at(410.0, 200.0));
        hover.track(at(398.0, 200.0));
        assert!(hover.track(at(398.0, 320.0)));
        assert!(!hover.open);
    }

    #[test]
    fn leaving_the_list_itself_closes_it() {
        let mut hover = hover();
        hover.track(at(410.0, 200.0));
        hover.track(at(200.0, 200.0));
        hover.track(at(60.0, 200.0));
        assert!(!hover.open);
    }
}
