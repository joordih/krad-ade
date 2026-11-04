use gpui_kit::{
    canvas, fill, point, px, rgb, size, Bounds, IntoElement, PathBuilder, Pixels, Styled, Window,
};
use krad_domain::{GraphRow, Lane, LaneColor, Segment};

pub(super) const LANE_W: f32 = 14.0;
const LINE_W: f32 = 1.5;
const NODE_D: f32 = 8.0;
const LANE_COLORS: [u32; 7] = [
    0x0052_8bff,
    0x00e0_7a3f,
    0x0040_b07a,
    0x00b0_6ad6,
    0x00d6_4f6a,
    0x00c9_a227,
    0x002f_a9c4,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GraphCell {
    Node(GraphRow),
    Through(Vec<(Lane, LaneColor)>),
}

impl GraphCell {
    pub(super) fn below(row: &GraphRow) -> Self {
        let mut lanes: Vec<(Lane, LaneColor)> = row
            .below
            .iter()
            .map(|segment| (segment.to, segment.color))
            .collect();
        lanes.sort_by_key(|(lane, _)| *lane);
        lanes.dedup_by_key(|(lane, _)| *lane);
        Self::Through(lanes)
    }
}

const fn lane_color(color: LaneColor) -> u32 {
    LANE_COLORS[color.0 % LANE_COLORS.len()]
}

#[allow(clippy::cast_precision_loss)]
fn lane_x(bounds: &Bounds<Pixels>, lane: Lane) -> f32 {
    bounds.origin.x.as_f32() + LANE_W * (lane.0 as f32 + 0.5)
}

fn paint_segment(
    bounds: &Bounds<Pixels>,
    segment: Segment,
    (from_y, to_y): (f32, f32),
    window: &mut Window,
) {
    let (from_x, to_x) = (lane_x(bounds, segment.from), lane_x(bounds, segment.to));
    let mut path = PathBuilder::stroke(px(LINE_W));
    path.move_to(point(px(from_x), px(from_y)));
    if segment.from == segment.to {
        path.line_to(point(px(to_x), px(to_y)));
    } else {
        let middle = f32::midpoint(from_y, to_y);
        path.cubic_bezier_to(
            point(px(to_x), px(to_y)),
            point(px(from_x), px(middle)),
            point(px(to_x), px(middle)),
        );
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(lane_color(segment.color)));
    }
}

fn paint_cell(cell: &GraphCell, bounds: &Bounds<Pixels>, window: &mut Window) {
    let top = bounds.origin.y.as_f32();
    let bottom = top + bounds.size.height.as_f32();
    let middle = f32::midpoint(top, bottom);
    match cell {
        GraphCell::Through(lanes) => {
            for (lane, color) in lanes {
                let segment = Segment {
                    from: *lane,
                    to: *lane,
                    color: *color,
                };
                paint_segment(bounds, segment, (top, bottom), window);
            }
        }
        GraphCell::Node(row) => {
            for segment in &row.above {
                paint_segment(bounds, *segment, (top, middle), window);
            }
            for segment in &row.below {
                paint_segment(bounds, *segment, (middle, bottom), window);
            }
            let center = lane_x(bounds, row.node);
            let node = Bounds::new(
                point(px(center - NODE_D / 2.0), px(middle - NODE_D / 2.0)),
                size(px(NODE_D), px(NODE_D)),
            );
            window
                .paint_quad(fill(node, rgb(lane_color(row.color))).corner_radii(px(NODE_D / 2.0)));
        }
    }
}

#[allow(clippy::cast_precision_loss)]
pub(super) fn graph_cell(cell: GraphCell, lanes: usize) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| paint_cell(&cell, &bounds, window),
    )
    .flex_none()
    .h_full()
    .w(px(LANE_W * lanes.max(1) as f32))
}
