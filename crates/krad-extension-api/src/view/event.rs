use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::id::{ActionId, NodeKey};
use crate::view::element::Sort;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViewEvent {
    Click {
        action: ActionId,
    },
    Input {
        action: ActionId,
        text: String,
    },
    Submit {
        action: ActionId,
        text: String,
    },
    Toggle {
        action: ActionId,
        value: bool,
    },
    Select {
        action: ActionId,
        value: String,
    },
    Form {
        action: ActionId,
        values: BTreeMap<NodeKey, FieldValue>,
    },
    Expand {
        item: NodeKey,
    },
    Collapse {
        item: NodeKey,
    },
    Choose {
        node: NodeKey,
        items: Vec<NodeKey>,
    },
    ItemAction {
        action: ActionId,
        item: NodeKey,
    },
    Sort {
        node: NodeKey,
        sort: Sort,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FieldValue {
    Toggle(bool),
    Number(f64),
    Text(String),
}
