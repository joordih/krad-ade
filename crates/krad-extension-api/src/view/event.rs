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
    SliderChange {
        action: ActionId,
        value: f64,
    },
    SliderCommit {
        action: ActionId,
        value: f64,
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

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn slider_events_carry_their_action_and_value() -> TestResult {
        let change = ViewEvent::SliderChange {
            action: "seek".parse()?,
            value: 42.5,
        };
        assert_eq!(
            serde_json::to_value(&change)?,
            serde_json::json!({"kind": "slider_change", "action": "seek", "value": 42.5})
        );
        let commit: ViewEvent =
            serde_json::from_str(r#"{"kind":"slider_commit","action":"volume","value":0.25}"#)?;
        assert_eq!(
            commit,
            ViewEvent::SliderCommit {
                action: "volume".parse()?,
                value: 0.25,
            }
        );
        Ok(())
    }
}
