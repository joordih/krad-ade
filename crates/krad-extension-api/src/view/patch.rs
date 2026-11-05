use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::NodeKey;
use crate::view::bounds::{clamp, depth_of, fits, item_measure_of, measure, Measure};
use crate::view::element::{Element, Node, StreamLine, TreeItem};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Patch {
    Replace {
        node: Node,
    },
    Insert {
        parent: NodeKey,
        index: u32,
        node: Node,
    },
    Remove {
        key: NodeKey,
    },
    SetCount {
        key: NodeKey,
        count: u32,
    },
    Invalidate {
        key: NodeKey,
    },
    ReplaceItem {
        tree: NodeKey,
        item: TreeItem,
    },
    Append {
        key: NodeKey,
        lines: Vec<StreamLine>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    Changed,
    RowsInvalidated(NodeKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PatchError {
    #[error("no node has the key {0}")]
    Missing(NodeKey),
    #[error("the node {0} cannot take this change")]
    Mismatch(NodeKey),
    #[error("the root cannot be removed")]
    Root,
    #[error("the view would grow past what the app draws")]
    TooLarge,
}

/// # Errors
///
/// Returns [`PatchError::TooLarge`] when the view holds more nodes or nests deeper than the app
/// draws. Sizes and columns are brought within their bounds.
pub fn bounded(node: &mut Node) -> Result<(), PatchError> {
    clamp(node);
    let Measure { nodes, height } = measure(node);
    if fits(nodes, height) {
        Ok(())
    } else {
        Err(PatchError::TooLarge)
    }
}

fn check_growth(
    root: &Node,
    anchor: &NodeKey,
    removed: usize,
    added: Measure,
) -> Result<(), PatchError> {
    let depth = depth_of(root, anchor).ok_or_else(|| PatchError::Missing(anchor.clone()))?;
    let total = measure(root)
        .nodes
        .saturating_sub(removed)
        .saturating_add(added.nodes);
    if fits(total, depth.saturating_add(added.height)) {
        Ok(())
    } else {
        Err(PatchError::TooLarge)
    }
}

fn find_ref<'a>(node: &'a Node, key: &NodeKey) -> Option<&'a Node> {
    if node.key == *key {
        return Some(node);
    }
    node.children()
        .iter()
        .find_map(|child| find_ref(child, key))
}

/// # Errors
///
/// Returns [`PatchError`] when the patch names a key the tree does not hold, or a node of another
/// kind than the change needs.
pub fn apply(root: &mut Node, patch: Patch) -> Result<Applied, PatchError> {
    match patch {
        Patch::Replace { mut node } => {
            let key = node.key.clone();
            clamp(&mut node);
            let removed = find_ref(root, &key).map_or(0, |old| measure(old).nodes);
            check_growth(root, &key, removed, measure(&node))?;
            *find(root, &key)? = node;
        }
        Patch::Insert {
            parent,
            index,
            mut node,
        } => {
            clamp(&mut node);
            let added = measure(&node);
            check_growth(
                root,
                &parent,
                0,
                Measure {
                    nodes: added.nodes,
                    height: added.height.saturating_add(1),
                },
            )?;
            let target = find(root, &parent)?;
            let children = target
                .children_mut()
                .ok_or_else(|| PatchError::Mismatch(parent.clone()))?;
            let at =
                usize::try_from(index).map_or(children.len(), |index| index.min(children.len()));
            children.insert(at, node);
        }
        Patch::Remove { key } => remove(root, &key)?,
        Patch::SetCount { key, count } => match &mut find(root, &key)?.element {
            Element::List { count: current, .. } | Element::Table { count: current, .. } => {
                *current = count;
                return Ok(Applied::RowsInvalidated(key));
            }
            _ => return Err(PatchError::Mismatch(key)),
        },
        Patch::Invalidate { key } => {
            return match find(root, &key)?.element {
                Element::List { .. } | Element::Table { .. } => Ok(Applied::RowsInvalidated(key)),
                _ => Err(PatchError::Mismatch(key)),
            };
        }
        Patch::ReplaceItem { tree, item } => {
            let added = item_measure_of(&item);
            let removed = find_ref(root, &tree)
                .and_then(|node| match &node.element {
                    Element::Tree { items } => find_item_ref(items, &item.key),
                    _ => None,
                })
                .map_or(0, |old| item_measure_of(old).nodes);
            check_growth(root, &tree, removed, added)?;
            let Element::Tree { items } = &mut find(root, &tree)?.element else {
                return Err(PatchError::Mismatch(tree));
            };
            let slot =
                find_item(items, &item.key).ok_or_else(|| PatchError::Missing(item.key.clone()))?;
            *slot = item;
        }
        Patch::Append { key, lines } => {
            let Element::Stream {
                lines: kept,
                max_lines,
            } = &mut find(root, &key)?.element
            else {
                return Err(PatchError::Mismatch(key));
            };
            kept.extend(lines);
            let limit = usize::try_from(*max_lines).unwrap_or(usize::MAX);
            let excess = kept.len().saturating_sub(limit);
            kept.drain(..excess);
        }
    }
    Ok(Applied::Changed)
}

fn find<'a>(node: &'a mut Node, key: &NodeKey) -> Result<&'a mut Node, PatchError> {
    if node.key == *key {
        return Ok(node);
    }
    node.children_mut()
        .into_iter()
        .flatten()
        .find_map(|child| find(child, key).ok())
        .ok_or_else(|| PatchError::Missing(key.clone()))
}

fn remove(node: &mut Node, key: &NodeKey) -> Result<(), PatchError> {
    if node.key == *key {
        return Err(PatchError::Root);
    }
    if remove_below(node, key) {
        Ok(())
    } else {
        Err(PatchError::Missing(key.clone()))
    }
}

fn remove_below(node: &mut Node, key: &NodeKey) -> bool {
    let Some(children) = node.children_mut() else {
        return false;
    };
    if let Some(at) = children.iter().position(|child| child.key == *key) {
        children.remove(at);
        return true;
    }
    children.iter_mut().any(|child| remove_below(child, key))
}

fn find_item_ref<'a>(items: &'a [TreeItem], key: &NodeKey) -> Option<&'a TreeItem> {
    items.iter().find_map(|item| {
        if item.key == *key {
            Some(item)
        } else {
            find_item_ref(&item.children, key)
        }
    })
}

fn find_item<'a>(items: &'a mut [TreeItem], key: &NodeKey) -> Option<&'a mut TreeItem> {
    items.iter_mut().find_map(|item| {
        if item.key == *key {
            Some(item)
        } else {
            find_item(&mut item.children, key)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::IdError;
    use crate::view::style::{Align, Direction, Justify, Style, TextStyle, Tone, Wrap};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn key(text: &str) -> Result<NodeKey, IdError> {
        text.parse()
    }

    fn text(name: &str, text: &str) -> Result<Node, IdError> {
        Ok(Node {
            key: key(name)?,
            element: Element::Text {
                text: text.to_owned(),
                style: TextStyle::default(),
            },
        })
    }

    fn column(name: &str, children: Vec<Node>) -> Result<Node, IdError> {
        Ok(Node {
            key: key(name)?,
            element: Element::Flex {
                direction: Direction::default(),
                align: Align::default(),
                justify: Justify::default(),
                wrap: Wrap::default(),
                style: Style::default(),
                children,
            },
        })
    }

    fn stream(name: &str, max_lines: u32) -> Result<Node, IdError> {
        Ok(Node {
            key: key(name)?,
            element: Element::Stream {
                lines: Vec::new(),
                max_lines,
            },
        })
    }

    fn keys(node: &Node) -> Vec<String> {
        node.children()
            .iter()
            .map(|child| child.key.to_string())
            .collect()
    }

    #[test]
    fn a_replaced_node_is_found_by_its_key_anywhere_in_the_tree() -> TestResult {
        let mut root = column("root", vec![column("inner", vec![text("title", "old")?])?])?;
        apply(
            &mut root,
            Patch::Replace {
                node: text("title", "new")?,
            },
        )?;
        assert_eq!(root.children()[0].children()[0], text("title", "new")?);
        Ok(())
    }

    #[test]
    fn inserts_land_at_their_index_or_at_the_end() -> TestResult {
        let mut root = column("root", vec![text("a", "a")?])?;
        for (name, index) in [("b", 0), ("c", 99)] {
            let node = text(name, name)?;
            apply(
                &mut root,
                Patch::Insert {
                    parent: key("root")?,
                    index,
                    node,
                },
            )?;
        }
        assert_eq!(keys(&root), ["b", "a", "c"]);
        Ok(())
    }

    #[test]
    fn removing_drops_the_node_but_never_the_root() -> TestResult {
        let mut root = column("root", vec![column("inner", vec![text("title", "x")?])?])?;
        apply(&mut root, Patch::Remove { key: key("title")? })?;
        assert_eq!(keys(&root.children()[0]), Vec::<String>::new());
        assert_eq!(
            apply(&mut root, Patch::Remove { key: key("root")? }),
            Err(PatchError::Root)
        );
        assert_eq!(
            apply(&mut root, Patch::Remove { key: key("gone")? }),
            Err(PatchError::Missing(key("gone")?))
        );
        Ok(())
    }

    #[test]
    fn a_stream_keeps_only_its_newest_lines() -> TestResult {
        let mut root = column("root", vec![stream("logs", 2)?])?;
        let lines = ["one", "two", "three"]
            .map(|text| StreamLine {
                text: text.to_owned(),
                tone: Tone::Neutral,
            })
            .to_vec();
        apply(
            &mut root,
            Patch::Append {
                key: key("logs")?,
                lines,
            },
        )?;
        let kept: Vec<&str> = match &root.children()[0].element {
            Element::Stream { lines, .. } => lines.iter().map(|line| line.text.as_str()).collect(),
            _ => Vec::new(),
        };
        assert_eq!(kept, ["two", "three"]);
        Ok(())
    }

    #[test]
    fn a_patch_for_another_kind_of_node_is_refused() -> TestResult {
        let mut root = column("root", vec![text("title", "x")?])?;
        assert_eq!(
            apply(
                &mut root,
                Patch::SetCount {
                    key: key("title")?,
                    count: 3
                }
            ),
            Err(PatchError::Mismatch(key("title")?))
        );
        Ok(())
    }

    fn nested(depth: usize) -> Result<Node, IdError> {
        let mut node = text("leaf", "x")?;
        for level in 0..depth {
            node = column(&format!("level-{level}"), vec![node])?;
        }
        Ok(node)
    }

    #[test]
    fn a_patch_that_nests_deeper_than_the_app_draws_is_refused() -> TestResult {
        let mut root = column("root", Vec::new())?;
        let deep = nested(crate::limits::MAX_VIEW_DEPTH + 1)?;
        assert_eq!(
            apply(
                &mut root,
                Patch::Insert {
                    parent: key("root")?,
                    index: 0,
                    node: deep
                }
            ),
            Err(PatchError::TooLarge)
        );
        assert_eq!(root.children().len(), 0);
        Ok(())
    }

    #[test]
    fn a_patch_that_floods_the_view_with_nodes_is_refused() -> TestResult {
        let many = (0..=crate::limits::MAX_VIEW_NODES)
            .map(|index| text(&format!("n{index}"), "x"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut root = column("root", Vec::new())?;
        let flood = column("flood", many)?;
        assert_eq!(
            apply(
                &mut root,
                Patch::Insert {
                    parent: key("root")?,
                    index: 0,
                    node: flood
                }
            ),
            Err(PatchError::TooLarge)
        );
        let mut big = column("root", Vec::new())?;
        big.children_mut()
            .ok_or("a column has children")?
            .push(column(
                "flood",
                (0..=crate::limits::MAX_VIEW_NODES)
                    .map(|index| text(&format!("n{index}"), "x"))
                    .collect::<Result<Vec<_>, _>>()?,
            )?);
        assert_eq!(bounded(&mut big), Err(PatchError::TooLarge));
        Ok(())
    }

    #[test]
    fn sizes_and_grid_columns_are_clamped_when_they_arrive() -> TestResult {
        use crate::view::element::MAX_GRID_COLUMNS;
        use crate::view::style::{Size, MAX_FIXED_SIZE};
        let mut root = column("root", Vec::new())?;
        let grid = Node {
            key: key("grid")?,
            element: Element::Grid {
                columns: u8::MAX,
                style: Style {
                    width: Size::Fixed(u16::MAX),
                    ..Style::default()
                },
                children: Vec::new(),
            },
        };
        apply(
            &mut root,
            Patch::Insert {
                parent: key("root")?,
                index: 0,
                node: grid,
            },
        )?;
        let Element::Grid { columns, style, .. } = &root.children()[0].element else {
            return Err("the grid was inserted".into());
        };
        assert_eq!(*columns, MAX_GRID_COLUMNS);
        assert_eq!(style.width, Size::Fixed(MAX_FIXED_SIZE));
        Ok(())
    }
}
