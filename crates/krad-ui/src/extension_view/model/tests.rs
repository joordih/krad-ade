use super::*;
use krad_extensions::api::view::{
    Align, Direction, Expansion, Justify, StreamLine, Style, TextStyle, Tone, TreeItem, Wrap,
};
use krad_extensions::api::IdError;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn key(text: &str) -> Result<NodeKey, IdError> {
    text.parse()
}

fn flex(name: &str, children: Vec<Node>) -> Result<Node, IdError> {
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

fn list_view() -> Result<Node, IdError> {
    let list = Node {
        key: key("containers")?,
        element: Element::List {
            count: 1000,
            selection: Selection::Many,
        },
    };
    flex("root", vec![list])
}

fn item(name: &str, expansion: Expansion, children: Vec<TreeItem>) -> Result<TreeItem, IdError> {
    Ok(TreeItem {
        key: key(name)?,
        label: name.to_owned(),
        description: None,
        icon: None,
        badge: None,
        expansion,
        children,
        actions: Vec::new(),
        menu: Vec::new(),
    })
}

fn tree_with(items: Vec<TreeItem>) -> Result<Node, IdError> {
    Ok(Node {
        key: key("root")?,
        element: Element::Tree { items },
    })
}

fn tree_view() -> Result<Node, IdError> {
    let web = item("web", Expansion::Leaf, Vec::new())?;
    tree_with(vec![item("compose", Expansion::Collapsed, vec![web])?])
}

fn text_row(name: &str) -> Result<ListRow, IdError> {
    Ok(ListRow {
        node: Node {
            key: key(name)?,
            element: Element::Text {
                text: name.to_owned(),
                style: TextStyle::default(),
            },
        },
        menu: Vec::new(),
    })
}

#[test]
fn a_visible_range_is_requested_once_and_filled_where_it_started() -> TestResult {
    let mut model = ViewModel::new(list_view()?);
    let list = key("containers")?;
    let request = model.rows_needed(&list, 0..20);
    assert_eq!(
        request,
        Some(ViewOutput::Rows {
            node: list.clone(),
            start: 0,
            end: 20
        })
    );
    assert_eq!(model.rows_needed(&list, 0..20), None);
    assert_eq!(
        model.rows_needed(&list, 10..30),
        Some(ViewOutput::Rows {
            node: list.clone(),
            start: 20,
            end: 30
        })
    );
    let rows = vec![text_row("a")?, text_row("b")?];
    model.set_rows(list.clone(), RowsResult::List { rows });
    assert!(matches!(model.row(&list, 1), Some(CachedRow::List(row)) if row.node.key == key("b")?));
    let later = vec![text_row("u")?];
    model.set_rows(list.clone(), RowsResult::List { rows: later });
    assert!(
        matches!(model.row(&list, 20), Some(CachedRow::List(row)) if row.node.key == key("u")?)
    );
    Ok(())
}

#[test]
fn a_request_never_exceeds_the_protocol_cap() -> TestResult {
    let mut model = ViewModel::new(list_view()?);
    let list = key("containers")?;
    let Some(ViewOutput::Rows { start, end, .. }) = model.rows_needed(&list, 0..5000) else {
        return Err("rows were expected".into());
    };
    assert_eq!(end - start, MAX_ROWS_PER_REQUEST);
    Ok(())
}

#[test]
fn invalidated_rows_are_asked_for_again() -> TestResult {
    let mut model = ViewModel::new(list_view()?);
    let list = key("containers")?;
    model.rows_needed(&list, 0..10);
    model.set_rows(
        list.clone(),
        RowsResult::List {
            rows: vec![text_row("a")?],
        },
    );
    model.apply(vec![Patch::Invalidate { key: list.clone() }]);
    assert!(model.row(&list, 0).is_none());
    assert!(model.rows_needed(&list, 0..10).is_some());
    Ok(())
}

#[test]
fn expanding_a_tree_item_shows_its_children_and_emits_expand() -> TestResult {
    let mut model = ViewModel::new(tree_view()?);
    let tree = key("root")?;
    assert_eq!(model.tree_rows(&tree).len(), 1);
    let event = model.toggle(&tree, &key("compose")?);
    assert_eq!(
        event,
        Some(ViewEvent::Expand {
            item: key("compose")?
        })
    );
    let rows = model.tree_rows(&tree);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].depth, 1);
    assert_eq!(
        model.toggle(&tree, &key("compose")?),
        Some(ViewEvent::Collapse {
            item: key("compose")?
        })
    );
    assert_eq!(model.toggle(&tree, &key("compose")?).map(|_| ()), Some(()));
    assert_eq!(model.toggle(&tree, &key("web")?), None);
    Ok(())
}

#[test]
fn a_replaced_tree_item_keeps_the_local_expansion() -> TestResult {
    let mut model = ViewModel::new(tree_view()?);
    let tree = key("root")?;
    model.toggle(&tree, &key("compose")?);
    let children = vec![
        item("web", Expansion::Leaf, Vec::new())?,
        item("db", Expansion::Leaf, Vec::new())?,
    ];
    let item = item("compose", Expansion::Collapsed, children)?;
    model.apply(vec![Patch::ReplaceItem {
        tree: tree.clone(),
        item,
    }]);
    assert_eq!(model.tree_rows(&tree).len(), 3);
    assert_eq!(model.tree_rows(&tree)[0].expansion, Shown::Open);
    Ok(())
}

#[test]
fn many_selection_toggles_and_single_selection_replaces() -> TestResult {
    let mut model = ViewModel::new(list_view()?);
    let list = key("containers")?;
    let (a, b) = (key("a")?, key("b")?);
    model.choose(&list, &a, Selection::Many);
    let event = model.choose(&list, &b, Selection::Many);
    assert_eq!(
        event,
        Some(ViewEvent::Choose {
            node: list.clone(),
            items: vec![a.clone(), b.clone()]
        })
    );
    model.choose(&list, &a, Selection::Many);
    assert!(!model.is_chosen(&list, &a));
    model.choose(&list, &a, Selection::Single);
    assert!(model.is_chosen(&list, &a) && !model.is_chosen(&list, &b));
    assert_eq!(model.choose(&list, &a, Selection::None), None);
    Ok(())
}

#[test]
fn appended_streams_are_reported_once() -> TestResult {
    let logs = Node {
        key: key("logs")?,
        element: Element::Stream {
            lines: Vec::new(),
            max_lines: 10,
        },
    };
    let root = flex("root", vec![logs])?;
    let mut model = ViewModel::new(root);
    let logs = key("logs")?;
    let lines = vec![StreamLine {
        text: "up".to_owned(),
        tone: Tone::Success,
    }];
    model.apply(vec![Patch::Append {
        key: logs.clone(),
        lines,
    }]);
    assert!(model.take_appended().contains(&logs));
    assert!(model.take_appended().is_empty());
    let mut keys = Vec::new();
    stream_keys(&model.root, &mut keys);
    assert_eq!(keys, [logs]);
    Ok(())
}
