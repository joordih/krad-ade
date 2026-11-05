use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::icon::Icon;
use crate::id::{ActionId, NodeKey, TerminalHandle};
use crate::view::style::{
    Align, ColorToken, Direction, Justify, Size, Style, TextStyle, Tone, Wrap,
};

pub const MAX_GRID_COLUMNS: u8 = 12;
pub const MAX_STREAM_LINES: u32 = 100_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Node {
    pub key: NodeKey,
    #[serde(flatten)]
    pub element: Element,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Element {
    Flex {
        #[serde(default)]
        direction: Direction,
        #[serde(default)]
        align: Align,
        #[serde(default)]
        justify: Justify,
        #[serde(default)]
        wrap: Wrap,
        #[serde(default)]
        style: Style,
        #[serde(default)]
        children: Vec<Node>,
    },
    Stack {
        #[serde(default)]
        style: Style,
        #[serde(default)]
        children: Vec<Node>,
    },
    Grid {
        columns: u8,
        #[serde(default)]
        style: Style,
        #[serde(default)]
        children: Vec<Node>,
    },
    Scroll {
        #[serde(default)]
        direction: Direction,
        #[serde(default)]
        style: Style,
        #[serde(default)]
        children: Vec<Node>,
    },
    Text {
        text: String,
        #[serde(default)]
        style: TextStyle,
    },
    Markdown {
        text: String,
    },
    Icon {
        icon: Icon,
        #[serde(default)]
        color: Option<ColorToken>,
    },
    Image {
        source: ImageSource,
        #[serde(default)]
        width: Size,
        #[serde(default)]
        height: Size,
    },
    Badge {
        text: String,
        #[serde(default)]
        tone: Tone,
    },
    Spinner,
    Divider,
    Button {
        action: ActionId,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        icon: Option<Icon>,
        #[serde(default)]
        variant: ButtonVariant,
        #[serde(default)]
        enabled: Enabled,
        #[serde(default)]
        tooltip: Option<String>,
    },
    Input {
        action: ActionId,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        kind: InputKind,
    },
    Toggle {
        action: ActionId,
        value: bool,
        #[serde(default)]
        label: Option<String>,
    },
    Select {
        action: ActionId,
        options: Vec<Choice>,
        #[serde(default)]
        value: Option<String>,
    },
    List {
        count: u32,
        #[serde(default)]
        selection: Selection,
    },
    Tree {
        items: Vec<TreeItem>,
    },
    Table {
        columns: Vec<Column>,
        count: u32,
        #[serde(default)]
        sort: Option<Sort>,
    },
    Form {
        submit: ActionId,
        fields: Vec<Field>,
        #[serde(default)]
        submit_label: Option<String>,
    },
    Details {
        entries: Vec<Detail>,
    },
    Stream {
        #[serde(default)]
        lines: Vec<StreamLine>,
        #[serde(default = "default_stream_lines")]
        max_lines: u32,
    },
    Terminal {
        terminal: TerminalHandle,
    },
}

const fn default_stream_lines() -> u32 {
    MAX_STREAM_LINES
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageSource {
    Bundled(PathBuf),
    Png(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Enabled {
    #[default]
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    #[default]
    Text,
    Multiline,
    Secret,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    #[default]
    None,
    Single,
    Many,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ItemAction {
    pub action: ActionId,
    pub label: String,
    #[serde(default)]
    pub icon: Option<Icon>,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Expansion {
    #[default]
    Leaf,
    Collapsed,
    Expanded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TreeItem {
    pub key: NodeKey,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<Icon>,
    #[serde(default)]
    pub badge: Option<Badge>,
    #[serde(default)]
    pub expansion: Expansion,
    #[serde(default)]
    pub children: Vec<TreeItem>,
    #[serde(default)]
    pub actions: Vec<ItemAction>,
    #[serde(default)]
    pub menu: Vec<ItemAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Badge {
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Column {
    pub key: NodeKey,
    pub title: String,
    #[serde(default)]
    pub width: Size,
    #[serde(default)]
    pub sortable: Sortable,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sortable {
    #[default]
    No,
    Yes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Sort {
    pub column: NodeKey,
    pub order: SortOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Field {
    pub key: NodeKey,
    pub label: String,
    #[serde(flatten)]
    pub kind: FieldKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Text {
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        kind: InputKind,
    },
    Toggle {
        #[serde(default)]
        value: bool,
    },
    Number {
        #[serde(default)]
        value: f64,
    },
    Select {
        options: Vec<Choice>,
        #[serde(default)]
        value: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Detail {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub style: TextStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StreamLine {
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TableRow {
    pub key: NodeKey,
    pub cells: Vec<Node>,
    #[serde(default)]
    pub menu: Vec<ItemAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListRow {
    pub node: Node,
    #[serde(default)]
    pub menu: Vec<ItemAction>,
}

impl Node {
    #[must_use]
    pub fn children(&self) -> &[Node] {
        match &self.element {
            Element::Flex { children, .. }
            | Element::Stack { children, .. }
            | Element::Grid { children, .. }
            | Element::Scroll { children, .. } => children,
            _ => &[],
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.element {
            Element::Flex { children, .. }
            | Element::Stack { children, .. }
            | Element::Grid { children, .. }
            | Element::Scroll { children, .. } => Some(children),
            _ => None,
        }
    }
}
