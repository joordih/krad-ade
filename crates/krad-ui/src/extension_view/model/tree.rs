use krad_extensions::api::view::{Badge, Expansion, ItemAction, TreeItem};
use krad_extensions::api::{Icon, NodeKey};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shown {
    Leaf,
    Closed,
    Open,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TreeRow {
    pub(crate) key: NodeKey,
    pub(crate) depth: usize,
    pub(crate) label: String,
    pub(crate) description: Option<String>,
    pub(crate) icon: Option<Icon>,
    pub(crate) badge: Option<Badge>,
    pub(crate) expansion: Shown,
    pub(crate) actions: Vec<ItemAction>,
    pub(crate) menu: Vec<ItemAction>,
}

fn shown(item: &TreeItem, expanded: &HashMap<NodeKey, bool>) -> Shown {
    if item.expansion == Expansion::Leaf {
        return Shown::Leaf;
    }
    let open = expanded
        .get(&item.key)
        .copied()
        .unwrap_or(item.expansion == Expansion::Expanded);
    if open {
        Shown::Open
    } else {
        Shown::Closed
    }
}

pub(crate) fn flatten(items: &[TreeItem], expanded: &HashMap<NodeKey, bool>) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    push_rows(items, 0, expanded, &mut rows);
    rows
}

fn push_rows(
    items: &[TreeItem],
    depth: usize,
    expanded: &HashMap<NodeKey, bool>,
    rows: &mut Vec<TreeRow>,
) {
    for item in items {
        let expansion = shown(item, expanded);
        rows.push(TreeRow {
            key: item.key.clone(),
            depth,
            label: item.label.clone(),
            description: item.description.clone(),
            icon: item.icon,
            badge: item.badge.clone(),
            expansion,
            actions: item.actions.clone(),
            menu: item.menu.clone(),
        });
        if expansion == Shown::Open {
            push_rows(&item.children, depth + 1, expanded, rows);
        }
    }
}
