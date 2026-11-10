use super::grid::{CanvasPoint, PaletteIndex, PixelGrid, CANVAS_SIZE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PaintTool {
    #[default]
    Brush,
    Eraser,
    Fill,
    Line,
    Rectangle,
    Ellipse,
}

impl PaintTool {
    pub const ALL: [Self; 6] = [
        Self::Brush,
        Self::Eraser,
        Self::Fill,
        Self::Line,
        Self::Rectangle,
        Self::Ellipse,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Eraser => "Eraser",
            Self::Fill => "Fill",
            Self::Line => "Line",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
        }
    }

    #[must_use]
    pub const fn drags_a_shape(self) -> bool {
        matches!(self, Self::Line | Self::Rectangle | Self::Ellipse)
    }

    pub fn apply(
        self,
        grid: &mut PixelGrid,
        from: CanvasPoint,
        to: CanvasPoint,
        color: PaletteIndex,
    ) {
        match self {
            Self::Brush | Self::Line => line(grid, from, to, Some(color)),
            Self::Eraser => line(grid, from, to, None),
            Self::Fill => flood(grid, to, Some(color)),
            Self::Rectangle => rectangle(grid, from, to, Some(color)),
            Self::Ellipse => ellipse(grid, from, to, Some(color)),
        }
    }
}

pub(super) fn line(
    grid: &mut PixelGrid,
    from: CanvasPoint,
    to: CanvasPoint,
    color: Option<PaletteIndex>,
) {
    let (mut x, mut y) = (i32::from(from.x), i32::from(from.y));
    let (end_x, end_y) = (i32::from(to.x), i32::from(to.y));
    let dx = (end_x - x).abs();
    let dy = -(end_y - y).abs();
    let step_x = if x < end_x { 1 } else { -1 };
    let step_y = if y < end_y { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        grid.set_signed(x, y, color);
        if x == end_x && y == end_y {
            return;
        }
        let doubled = 2 * error;
        if doubled >= dy {
            error += dy;
            x += step_x;
        }
        if doubled <= dx {
            error += dx;
            y += step_y;
        }
    }
}

pub(super) fn rectangle(
    grid: &mut PixelGrid,
    from: CanvasPoint,
    to: CanvasPoint,
    color: Option<PaletteIndex>,
) {
    let (left, right) = (from.x.min(to.x), from.x.max(to.x));
    let (top, bottom) = (from.y.min(to.y), from.y.max(to.y));
    let corner = |x, y| CanvasPoint { x, y };
    line(grid, corner(left, top), corner(right, top), color);
    line(grid, corner(left, bottom), corner(right, bottom), color);
    line(grid, corner(left, top), corner(left, bottom), color);
    line(grid, corner(right, top), corner(right, bottom), color);
}

pub(super) fn filled_ellipse(
    grid: &mut PixelGrid,
    from: CanvasPoint,
    to: CanvasPoint,
    color: Option<PaletteIndex>,
) {
    each_in_ellipse(from, to, |x, y, _| grid.set_signed(x, y, color));
}

fn ellipse(grid: &mut PixelGrid, from: CanvasPoint, to: CanvasPoint, color: Option<PaletteIndex>) {
    each_in_ellipse(from, to, |x, y, on_edge| {
        if on_edge {
            grid.set_signed(x, y, color);
        }
    });
}

fn each_in_ellipse(from: CanvasPoint, to: CanvasPoint, mut visit: impl FnMut(i32, i32, bool)) {
    let (left, right) = (i32::from(from.x.min(to.x)), i32::from(from.x.max(to.x)));
    let (top, bottom) = (i32::from(from.y.min(to.y)), i32::from(from.y.max(to.y)));
    let inside = |x: i32, y: i32| {
        if x < left || x > right || y < top || y > bottom {
            return false;
        }
        let radius_x = f64::from(right - left + 1) / 2.0;
        let radius_y = f64::from(bottom - top + 1) / 2.0;
        let centre_x = f64::from(left) + radius_x;
        let centre_y = f64::from(top) + radius_y;
        let nx = (f64::from(x) + 0.5 - centre_x) / radius_x;
        let ny = (f64::from(y) + 0.5 - centre_y) / radius_y;
        nx * nx + ny * ny <= 1.0
    };
    for y in top..=bottom {
        for x in left..=right {
            if !inside(x, y) {
                continue;
            }
            let on_edge =
                !(inside(x - 1, y) && inside(x + 1, y) && inside(x, y - 1) && inside(x, y + 1));
            visit(x, y, on_edge);
        }
    }
}

fn flood(grid: &mut PixelGrid, at: CanvasPoint, color: Option<PaletteIndex>) {
    let replaced = grid.get(at);
    if replaced == color {
        return;
    }
    let mut pending = vec![at];
    while let Some(point) = pending.pop() {
        if grid.get(point) != replaced {
            continue;
        }
        grid.set(point, color);
        let (x, y) = (point.x, point.y);
        let neighbours = [
            x.checked_sub(1).map(|x| (x, y)),
            (x + 1 < CANVAS_SIZE).then_some((x + 1, y)),
            y.checked_sub(1).map(|y| (x, y)),
            (y + 1 < CANVAS_SIZE).then_some((x, y + 1)),
        ];
        pending.extend(
            neighbours
                .into_iter()
                .flatten()
                .map(|(x, y)| CanvasPoint { x, y }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: u8, y: u8) -> CanvasPoint {
        CanvasPoint::new(x, y).unwrap_or_else(|| unreachable!())
    }

    fn ink() -> PaletteIndex {
        PaletteIndex::try_from(2_u8).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn a_brush_stroke_leaves_a_connected_line_and_the_eraser_lifts_it() {
        let mut grid = PixelGrid::default();
        PaintTool::Brush.apply(&mut grid, point(0, 0), point(5, 2), ink());
        assert_eq!(grid.painted().count(), 6);
        assert_eq!(grid.get(point(5, 2)), Some(ink()));
        PaintTool::Eraser.apply(&mut grid, point(0, 0), point(5, 2), ink());
        assert_eq!(grid.painted().count(), 0);
    }

    #[test]
    fn fill_stays_inside_an_outline() {
        let mut grid = PixelGrid::default();
        PaintTool::Rectangle.apply(&mut grid, point(2, 2), point(6, 6), ink());
        assert_eq!(grid.painted().count(), 16);
        let inner = PaletteIndex::try_from(3_u8).unwrap_or_else(|_| unreachable!());
        PaintTool::Fill.apply(&mut grid, point(4, 4), point(4, 4), inner);
        assert_eq!(
            grid.painted().filter(|(_, color)| *color == inner).count(),
            9
        );
        assert_eq!(grid.get(point(0, 0)), None);
        PaintTool::Fill.apply(&mut grid, point(0, 0), point(0, 0), inner);
        assert_eq!(
            grid.painted().filter(|(_, color)| *color == inner).count(),
            576 - 16
        );
    }

    #[test]
    fn an_ellipse_outline_is_symmetric_and_hollow() {
        let mut grid = PixelGrid::default();
        PaintTool::Ellipse.apply(&mut grid, point(4, 4), point(12, 10), ink());
        assert_eq!(grid.get(point(8, 7)), None);
        assert_eq!(grid.get(point(4, 7)), Some(ink()));
        assert_eq!(grid.get(point(12, 7)), Some(ink()));
        assert_eq!(grid.get(point(8, 4)), Some(ink()));
        assert_eq!(grid.get(point(8, 10)), Some(ink()));
    }
}
