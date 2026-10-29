use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolDecision {
    Pass,
    Allow {
        updated_input: Option<Value>,
        permissions: Vec<Value>,
    },
    Deny {
        message: String,
        interrupt: bool,
    },
}
