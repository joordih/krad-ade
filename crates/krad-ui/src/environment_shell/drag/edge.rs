use gpui_kit::base::{resize_handle, ResizeHandle, ResizeHandleRenderer, ResizeHandleState};
use gpui_kit::component::resize_handle_appearance;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, App, AppContext as _, Bounds, Context, DragMoveEvent, ElementId, Empty, IntoElement,
    ParentElement as _, Pixels, Point, Render, Styled as _, Window,
};
use krad_domain::{Axis, DividerId, UnitRect};
use std::rc::Rc;

use super::DragState;
use crate::environment_shell::EnvironmentShell;

const MIN_SPAN_PX: f32 = 1.0;
const HIDDEN: f32 = 0.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::environment_shell) enum EdgeDrag {
    Sidebar,
    Viewer,
    SidePanel,
    History,
    Activity,
    Pane {
        divider: DividerId,
        axis: Axis,
        frame: UnitRect,
    },
}

impl EdgeDrag {
    pub(in crate::environment_shell) const fn reshapes_layout(self) -> bool {
        matches!(self, Self::Sidebar | Self::SidePanel | Self::History)
    }
}

impl Render for EdgeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum FixedSide {
    Left,
    Right,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::environment_shell) struct EdgeMove {
    bounds: Bounds<Pixels>,
    pointer: Point<Pixels>,
}

impl EdgeMove {
    pub(in crate::environment_shell) fn extent_from(self, side: FixedSide) -> f32 {
        let extent = match side {
            FixedSide::Left => self.pointer.x - self.bounds.left(),
            FixedSide::Right => self.bounds.right() - self.pointer.x,
            FixedSide::Bottom => self.bounds.bottom() - self.pointer.y,
        };
        extent.as_f32()
    }

    pub(in crate::environment_shell) fn ratio_across(self, axis: Axis, frame: UnitRect) -> f32 {
        let offset = self.pointer - self.bounds.origin;
        let size = self.bounds.size;
        match axis {
            Axis::Row => {
                (offset.x.as_f32() / size.width.as_f32().max(MIN_SPAN_PX) - frame.x)
                    / frame.w.max(f32::EPSILON)
            }
            Axis::Column => {
                (offset.y.as_f32() / size.height.as_f32().max(MIN_SPAN_PX) - frame.y)
                    / frame.h.max(f32::EPSILON)
            }
        }
    }
}

pub(in crate::environment_shell) fn edge_handle(
    id: impl Into<ElementId>,
    axis: gpui_kit::Axis,
    edge: EdgeDrag,
) -> ResizeHandle<EdgeDrag, EdgeDrag> {
    resize_handle(id, axis)
        .with_appearance(quiet_at_rest())
        .on_drag(edge, |edge, _, _, cx| cx.new(|_| *edge))
}

fn quiet_at_rest() -> ResizeHandleRenderer {
    let divider = resize_handle_appearance();
    Rc::new(move |handle, window, cx| {
        let drawn = divider(handle, window, cx)?;
        Some(
            div()
                .flex_none()
                .size_full()
                .when(handle.state() == ResizeHandleState::Idle, |line| {
                    line.opacity(HIDDEN)
                })
                .child(drawn)
                .into_any_element(),
        )
    })
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn follow_edge(
        cx: &mut Context<Self>,
        resize: impl Fn(&mut Self, EdgeDrag, EdgeMove, &mut Context<Self>) -> Option<()> + 'static,
    ) -> impl Fn(&DragMoveEvent<EdgeDrag>, &mut Window, &mut App) + 'static {
        let shell = cx.entity().downgrade();
        move |event, _, cx| {
            let edge = *event.drag(cx);
            let moved = EdgeMove {
                bounds: event.bounds,
                pointer: event.event.position,
            };
            shell
                .update(cx, |this, cx| {
                    if resize(this, edge, moved, cx).is_some() {
                        this.drag = DragState::Resize(edge);
                        cx.notify();
                    }
                })
                .unwrap_or_default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    fn moved(x: f32, y: f32) -> EdgeMove {
        EdgeMove {
            bounds: Bounds {
                origin: point(px(100.0), px(50.0)),
                size: size(px(400.0), px(200.0)),
            },
            pointer: point(px(x), px(y)),
        }
    }

    fn same(a: f32, b: f32) -> bool {
        (a - b).abs() < f32::EPSILON
    }

    #[test]
    fn a_trailing_edge_follows_the_pointer_from_the_fixed_left_side() {
        assert!(same(moved(340.0, 0.0).extent_from(FixedSide::Left), 240.0));
    }

    #[test]
    fn a_leading_edge_widens_its_card_as_the_pointer_goes_left() {
        assert!(same(moved(300.0, 0.0).extent_from(FixedSide::Right), 200.0));
        assert!(same(moved(200.0, 0.0).extent_from(FixedSide::Right), 300.0));
    }

    #[test]
    fn a_top_edge_grows_its_panel_as_the_pointer_goes_up() {
        assert!(same(
            moved(0.0, 150.0).extent_from(FixedSide::Bottom),
            100.0
        ));
        assert!(same(moved(0.0, 90.0).extent_from(FixedSide::Bottom), 160.0));
    }

    #[test]
    fn a_divider_ratio_is_measured_inside_the_frame_it_splits() {
        let frame = UnitRect {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        assert!(same(moved(400.0, 0.0).ratio_across(Axis::Row, frame), 0.5));
        let rows = UnitRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 0.5,
        };
        assert!(same(
            moved(0.0, 150.0).ratio_across(Axis::Column, rows),
            1.0
        ));
    }

    #[test]
    fn only_sidebar_and_side_panel_edges_save_the_layout() {
        assert!(EdgeDrag::Sidebar.reshapes_layout());
        assert!(EdgeDrag::SidePanel.reshapes_layout());
        assert!(EdgeDrag::History.reshapes_layout());
        assert!(!EdgeDrag::Activity.reshapes_layout());
        assert!(!EdgeDrag::Viewer.reshapes_layout());
    }
}
