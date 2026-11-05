mod tree;

pub(crate) use tree::{Shown, TreeRow};

use krad_extensions::api::protocol::{RowsResult, MAX_ROWS_PER_REQUEST};
use krad_extensions::api::view::{
    apply, Applied, Element, ListRow, Node, Patch, Selection, TableRow, ViewEvent,
};
use krad_extensions::api::NodeKey;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ops::Range;

use super::ViewOutput;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CachedRow {
    List(ListRow),
    Table(TableRow),
}

#[derive(Debug, Default)]
struct RowStore {
    rows: BTreeMap<u32, CachedRow>,
    requested: Vec<Range<u32>>,
    starts: VecDeque<u32>,
}

impl RowStore {
    fn is_known(&self, index: u32) -> bool {
        self.rows.contains_key(&index) || self.requested.iter().any(|range| range.contains(&index))
    }
}

#[derive(Debug)]
pub(crate) struct ViewModel {
    pub(crate) root: Node,
    rows: HashMap<NodeKey, RowStore>,
    expanded: HashMap<NodeKey, bool>,
    trees: HashMap<NodeKey, Vec<TreeRow>>,
    selection: HashMap<NodeKey, Vec<NodeKey>>,
    appended: HashSet<NodeKey>,
    received_sliders: HashSet<NodeKey>,
}

impl ViewModel {
    pub(crate) fn new(root: Node) -> Self {
        let mut model = Self {
            root,
            rows: HashMap::new(),
            expanded: HashMap::new(),
            trees: HashMap::new(),
            selection: HashMap::new(),
            appended: HashSet::new(),
            received_sliders: HashSet::new(),
        };
        slider_keys(&model.root, &mut model.received_sliders);
        model.reflatten();
        model
    }

    pub(crate) fn replace_root(&mut self, root: Node) {
        *self = Self::new(root);
    }

    pub(crate) fn apply(&mut self, patches: Vec<Patch>) {
        for patch in patches {
            let appended = match &patch {
                Patch::Append { key, .. } => Some(key.clone()),
                _ => None,
            };
            let mut sliders = HashSet::new();
            if let Patch::Replace { node } | Patch::Insert { node, .. } = &patch {
                slider_keys(node, &mut sliders);
            }
            match apply(&mut self.root, patch) {
                Ok(Applied::Changed) => {
                    self.appended.extend(appended);
                    self.received_sliders.extend(sliders);
                }
                Ok(Applied::RowsInvalidated(node)) => {
                    self.rows.remove(&node);
                }
                Err(error) => tracing::debug!(%error, "extension view patch refused"),
            }
        }
        self.reflatten();
    }

    pub(crate) fn take_appended(&mut self) -> HashSet<NodeKey> {
        std::mem::take(&mut self.appended)
    }

    pub(crate) fn take_received_sliders(&mut self) -> HashSet<NodeKey> {
        std::mem::take(&mut self.received_sliders)
    }

    fn reflatten(&mut self) {
        self.trees.clear();
        let mut found = Vec::new();
        collect_trees(&self.root, &mut found);
        for (key, rows) in found
            .into_iter()
            .map(|(key, items)| (key, tree::flatten(items, &self.expanded)))
        {
            self.trees.insert(key, rows);
        }
    }

    pub(crate) fn rows_needed(
        &mut self,
        node: &NodeKey,
        visible: Range<u32>,
    ) -> Option<ViewOutput> {
        let store = self.rows.entry(node.clone()).or_default();
        let mut missing = visible.filter(|index| !store.is_known(*index));
        let start = missing.next()?;
        let end = missing
            .next_back()
            .unwrap_or(start)
            .saturating_add(1)
            .min(start.saturating_add(MAX_ROWS_PER_REQUEST));
        store.requested.push(start..end);
        store.starts.push_back(start);
        Some(ViewOutput::Rows {
            node: node.clone(),
            start,
            end,
        })
    }

    pub(crate) fn set_rows(&mut self, node: NodeKey, rows: RowsResult) {
        let store = self.rows.entry(node).or_default();
        let start = store.starts.pop_front().unwrap_or_default();
        store.requested.retain(|range| range.start != start);
        let cached: Vec<CachedRow> = match rows {
            RowsResult::List { rows } => rows.into_iter().map(CachedRow::List).collect(),
            RowsResult::Table { rows } => rows.into_iter().map(CachedRow::Table).collect(),
        };
        for (index, row) in (start..).zip(cached) {
            store.rows.insert(index, row);
        }
    }

    pub(crate) fn row(&self, node: &NodeKey, index: u32) -> Option<&CachedRow> {
        self.rows.get(node)?.rows.get(&index)
    }

    pub(crate) fn tree_rows(&self, tree: &NodeKey) -> &[TreeRow] {
        self.trees.get(tree).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn toggle(&mut self, tree: &NodeKey, item: &NodeKey) -> Option<ViewEvent> {
        let row = self.tree_rows(tree).iter().find(|row| row.key == *item)?;
        let event = match row.expansion {
            tree::Shown::Leaf => return None,
            tree::Shown::Open => ViewEvent::Collapse { item: item.clone() },
            tree::Shown::Closed => ViewEvent::Expand { item: item.clone() },
        };
        let opens = matches!(event, ViewEvent::Expand { .. });
        self.expanded.insert(item.clone(), opens);
        self.reflatten();
        Some(event)
    }

    pub(crate) fn choose(
        &mut self,
        node: &NodeKey,
        item: &NodeKey,
        mode: Selection,
    ) -> Option<ViewEvent> {
        let chosen = self.selection.entry(node.clone()).or_default();
        match mode {
            Selection::None => return None,
            Selection::Single => {
                chosen.clear();
                chosen.push(item.clone());
            }
            Selection::Many => {
                if let Some(at) = chosen.iter().position(|known| known == item) {
                    chosen.remove(at);
                } else {
                    chosen.push(item.clone());
                }
            }
        }
        Some(ViewEvent::Choose {
            node: node.clone(),
            items: chosen.clone(),
        })
    }

    pub(crate) fn is_chosen(&self, node: &NodeKey, item: &NodeKey) -> bool {
        self.selection
            .get(node)
            .is_some_and(|chosen| chosen.contains(item))
    }
}

pub(crate) fn find<'a>(node: &'a Node, key: &NodeKey) -> Option<&'a Node> {
    if node.key == *key {
        return Some(node);
    }
    node.children().iter().find_map(|child| find(child, key))
}

pub(crate) fn slider_keys(node: &Node, keys: &mut HashSet<NodeKey>) {
    if let Element::Slider(_) = node.element {
        keys.insert(node.key.clone());
    }
    for child in node.children() {
        slider_keys(child, keys);
    }
}

pub(crate) fn stream_keys(node: &Node, keys: &mut Vec<NodeKey>) {
    if let Element::Stream { .. } = node.element {
        keys.push(node.key.clone());
    }
    for child in node.children() {
        stream_keys(child, keys);
    }
}

fn collect_trees<'a>(
    node: &'a Node,
    found: &mut Vec<(NodeKey, &'a [krad_extensions::api::view::TreeItem])>,
) {
    if let Element::Tree { items } = &node.element {
        found.push((node.key.clone(), items));
    }
    for child in node.children() {
        collect_trees(child, found);
    }
}

#[cfg(test)]
mod tests;
