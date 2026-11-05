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
