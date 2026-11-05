use crate::id::NodeKey;
use crate::limits::{MAX_VIEW_DEPTH, MAX_VIEW_NODES};
use crate::view::element::{Column, Element, Node, TreeItem, MAX_GRID_COLUMNS, MAX_STREAM_LINES};
use crate::view::style::Style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Measure {
    pub(crate) nodes: usize,
    pub(crate) height: usize,
}

fn item_measure(item: &TreeItem) -> Measure {
    item.children.iter().map(item_measure).fold(
        Measure {
            nodes: 1,
            height: 1,
        },
        |total, child| Measure {
            nodes: total.nodes.saturating_add(child.nodes),
            height: total.height.max(child.height.saturating_add(1)),
        },
    )
}

pub(crate) fn measure(node: &Node) -> Measure {
    let own = match &node.element {
        Element::Tree { items } => items.iter().map(item_measure).fold(
            Measure {
                nodes: 1,
                height: 1,
            },
            |total, item| Measure {
                nodes: total.nodes.saturating_add(item.nodes),
                height: total.height.max(item.height.saturating_add(1)),
            },
        ),
        _ => Measure {
            nodes: 1,
            height: 1,
        },
    };
    node.children()
        .iter()
        .map(measure)
        .fold(own, |total, child| Measure {
            nodes: total.nodes.saturating_add(child.nodes),
            height: total.height.max(child.height.saturating_add(1)),
        })
}

pub(crate) fn item_measure_of(item: &TreeItem) -> Measure {
    item_measure(item)
}

pub(crate) fn depth_of(node: &Node, key: &NodeKey) -> Option<usize> {
    if node.key == *key {
        return Some(0);
    }
    node.children()
        .iter()
        .find_map(|child| depth_of(child, key).map(|depth| depth + 1))
}

pub(crate) fn fits(nodes: usize, height: usize) -> bool {
    nodes <= MAX_VIEW_NODES && height <= MAX_VIEW_DEPTH
}

fn clamp_style(style: &mut Style) {
    style.width = style.width.bounded();
    style.height = style.height.bounded();
}

fn clamp_column(column: &mut Column) {
    column.width = column.width.bounded();
}

pub(crate) fn clamp(node: &mut Node) {
    match &mut node.element {
        Element::Flex { style, .. }
        | Element::Stack { style, .. }
        | Element::Scroll { style, .. } => {
            clamp_style(style);
        }
        Element::Grid { columns, style, .. } => {
            *columns = (*columns).clamp(1, MAX_GRID_COLUMNS);
            clamp_style(style);
        }
        Element::Image { width, height, .. } => {
            *width = width.bounded();
            *height = height.bounded();
        }
        Element::Table { columns, .. } => columns.iter_mut().for_each(clamp_column),
        Element::Stream { max_lines, .. } => *max_lines = (*max_lines).min(MAX_STREAM_LINES),
        _ => {}
    }
    if let Some(children) = node.children_mut() {
        children.iter_mut().for_each(clamp);
    }
}
