use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::NodeKey;
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
}

/// # Errors
///
/// Returns [`PatchError`] when the patch names a key the tree does not hold, or a node of another
/// kind than the change needs.
pub fn apply(root: &mut Node, patch: Patch) -> Result<Applied, PatchError> {
    match patch {
        Patch::Replace { node } => {
            let key = node.key.clone();
            *find(root, &key)? = node;
        }
        Patch::Insert {
            parent,
            index,
            node,
        } => {
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
}
