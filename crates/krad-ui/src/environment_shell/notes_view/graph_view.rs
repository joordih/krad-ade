use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    canvas, div, point, px, rgb, AnyElement, Bounds, Context, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, PathBuilder, Pixels,
    ScrollWheelEvent, StatefulInteractiveElement, Styled,
};
use krad_domain::{FileOpening, GraphLayout, NoteGraph};

use super::super::EnvironmentShell;
use crate::theme::{palette, tinted, LINE_XS, RADIUS_XS, TEXT_XS};

const STEPS_PER_FRAME: usize = 6;
const NODE_MIN: f32 = 6.0;
const NODE_PER_LINK: f32 = 1.5;
const NODE_MAX: f32 = 18.0;
const EDGE_WIDTH: f32 = 1.0;
const EDGE_ALPHA: u32 = 0x55;
const LABEL_W: f32 = 140.0;
const LABEL_GAP: f32 = 4.0;
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 4.0;
const ZOOM_STEP: f32 = 0.0015;
const LABEL_ZOOM: f32 = 0.6;
const LOCAL_HOPS: usize = 2;
const HALF: f32 = 0.5;
const LOCAL_H: f32 = 220.0;
const GRAPH_NODE_PREFIX: &str = "graph-node";
const LOCAL_NODE_PREFIX: &str = "local-graph-node";
const PATH_SEPARATOR: char = '/';
const NOTE_SUFFIX: &str = ".md";

struct DrawnGraph {
    graph: NoteGraph,
    layout: GraphLayout,
    settled: bool,
    pan: (f32, f32),
    zoom: f32,
}

impl DrawnGraph {
    fn new(graph: NoteGraph) -> Self {
        let layout = GraphLayout::new(graph.nodes.len());
        Self {
            graph,
            layout,
            settled: false,
            pan: (0.0, 0.0),
            zoom: 1.0,
        }
    }

    fn advance(&mut self) {
        for _ in 0..STEPS_PER_FRAME {
            if self.settled {
                return;
            }
            self.settled = self.layout.step(&self.graph);
        }
    }
}

#[derive(Clone, Copy)]
struct NodePlace {
    at: (f32, f32),
    size: f32,
    pan: (f32, f32),
    zoom: f32,
}

#[derive(Default)]
pub(in crate::environment_shell) struct GraphState {
    pub(super) open: bool,
    pub(super) showing: bool,
    vault: Option<DrawnGraph>,
    local: Option<(String, DrawnGraph)>,
    drag: Option<(f32, f32)>,
}

impl GraphState {
    pub(super) fn forget(&mut self) {
        self.vault = None;
        self.local = None;
    }
}

fn node_size(links: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let size = NODE_MIN + links as f32 * NODE_PER_LINK;
    size.min(NODE_MAX)
}

fn label_of(path: &str) -> String {
    let name = path.rsplit(PATH_SEPARATOR).next().unwrap_or(path);
    name.strip_suffix(NOTE_SUFFIX).unwrap_or(name).to_owned()
}

impl EnvironmentShell {
    pub(super) fn open_vault_graph(&mut self, cx: &mut Context<Self>) {
        self.notes.graph.open = true;
        self.notes.graph.showing = true;
        cx.notify();
    }

    pub(super) fn vault_graph(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let index = &self.notes.index;
        let drawn = self
            .notes
            .graph
            .vault
            .get_or_insert_with(|| DrawnGraph::new(NoteGraph::of_vault(index)));
        drawn.advance();
        let settled = drawn.settled;
        let view = self.graph_view(false, cx);
        if !settled {
            cx.notify();
        }
        div()
            .id("vault-graph")
            .flex()
            .flex_1()
            .min_h_0()
            .on_scroll_wheel(
                cx.listener(|this: &mut Self, event: &ScrollWheelEvent, _, cx| {
                    if let Some(drawn) = this.notes.graph.vault.as_mut() {
                        let delta = event.delta.pixel_delta(px(LINE_XS)).y.as_f32();
                        drawn.zoom =
                            (drawn.zoom * (1.0 - delta * ZOOM_STEP)).clamp(ZOOM_MIN, ZOOM_MAX);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                    this.notes.graph.drag =
                        Some((event.position.x.as_f32(), event.position.y.as_f32()));
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.notes.graph.drag = None;
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(|this: &mut Self, event: &MouseMoveEvent, _, cx| {
                    let Some((x, y)) = this.notes.graph.drag else {
                        return;
                    };
                    let (nx, ny) = (event.position.x.as_f32(), event.position.y.as_f32());
                    if let Some(drawn) = this.notes.graph.vault.as_mut() {
                        drawn.pan.0 += nx - x;
                        drawn.pan.1 += ny - y;
                    }
                    this.notes.graph.drag = Some((nx, ny));
                    cx.notify();
                }),
            )
            .child(view)
            .into_any_element()
    }

    pub(super) fn local_graph(&mut self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let stale = self
            .notes
            .graph
            .local
            .as_ref()
            .is_none_or(|(shown, _)| shown != path);
        if stale {
            let graph = NoteGraph::of_vault(&self.notes.index).around(path, LOCAL_HOPS);
            self.notes.graph.local = Some((path.to_owned(), DrawnGraph::new(graph)));
        }
        let settled = self.notes.graph.local.as_mut().is_some_and(|(_, drawn)| {
            drawn.advance();
            drawn.settled
        });
        if !settled {
            cx.notify();
        }
        div()
            .flex()
            .flex_none()
            .h(px(LOCAL_H))
            .child(self.graph_view(true, cx))
            .into_any_element()
    }

    fn note_graph_node(
        index: usize,
        path: &str,
        place: NodePlace,
        local: bool,
        current: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let NodePlace {
            at: (x, y),
            size,
            pan,
            zoom,
        } = place;
        let prefix = if local {
            LOCAL_NODE_PREFIX
        } else {
            GRAPH_NODE_PREFIX
        };
        let opened = path.to_owned();
        let show_label = local || zoom >= LABEL_ZOOM;
        div()
            .id((prefix, index))
            .absolute()
            .left(gpui_kit::relative(HALF))
            .top(gpui_kit::relative(HALF))
            .ml(px(x * zoom + pan.0 - LABEL_W / 2.0))
            .mt(px(y * zoom + pan.1 - size / 2.0))
            .w(px(LABEL_W))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(LABEL_GAP))
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.notes.graph.showing = false;
                this.open_vault_note(&opened, FileOpening::Preview, cx);
            }))
            .child(
                div()
                    .size(px(size))
                    .rounded_full()
                    .bg(rgb(if current { pal.accent } else { pal.text_muted }))
                    .hover(|style| style.bg(rgb(pal.accent))),
            )
            .when(show_label, |node| {
                node.child(
                    div()
                        .px(px(LABEL_GAP))
                        .rounded(px(RADIUS_XS))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(if current {
                            pal.text_strong
                        } else {
                            pal.text_default
                        }))
                        .truncate()
                        .child(label_of(path)),
                )
            })
            .into_any_element()
    }

    fn graph_view(&self, local: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let drawn = if local {
            self.notes.graph.local.as_ref().map(|(_, drawn)| drawn)
        } else {
            self.notes.graph.vault.as_ref()
        };
        let Some(drawn) = drawn else {
            return div().into_any_element();
        };
        let open = self
            .note_tabs()
            .and_then(|tabs| tabs.active_tab())
            .map(|tab| tab.path.clone());
        let degrees = drawn.graph.degrees();
        let positions = drawn.layout.positions.clone();
        let edges = drawn.graph.edges.clone();
        let (pan, zoom) = if local {
            ((0.0, 0.0), 1.0)
        } else {
            (drawn.pan, drawn.zoom)
        };
        let edge_color = tinted(pal.text_muted, EDGE_ALPHA);
        let lines = canvas(
            move |_, _, _| (),
            move |bounds: Bounds<Pixels>, (), window, _| {
                let center = bounds.center();
                let at = |(x, y): (f32, f32)| {
                    point(
                        center.x + px(x * zoom + pan.0),
                        center.y + px(y * zoom + pan.1),
                    )
                };
                for (a, b) in &edges {
                    let (Some(from), Some(to)) = (positions.get(*a), positions.get(*b)) else {
                        continue;
                    };
                    let mut path = PathBuilder::stroke(px(EDGE_WIDTH));
                    path.move_to(at(*from));
                    path.line_to(at(*to));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, edge_color);
                    }
                }
            },
        )
        .absolute()
        .size_full();
        let nodes: Vec<AnyElement> = drawn
            .graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let place = NodePlace {
                    at: drawn
                        .layout
                        .positions
                        .get(index)
                        .copied()
                        .unwrap_or_default(),
                    size: node_size(degrees.get(index).copied().unwrap_or_default()),
                    pan,
                    zoom,
                };
                let current = open.as_deref() == Some(path.as_str());
                Self::note_graph_node(index, path, place, local, current, cx)
            })
            .collect();
        div()
            .relative()
            .flex_1()
            .size_full()
            .overflow_hidden()
            .child(lines)
            .children(nodes)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_grow_with_links_up_to_a_limit() {
        assert!((node_size(0) - NODE_MIN).abs() < f32::EPSILON);
        assert!(node_size(2) > node_size(1));
        assert!((node_size(100) - NODE_MAX).abs() < f32::EPSILON);
        assert_eq!(label_of("Ideas/Spark.md"), "Spark");
    }
}
