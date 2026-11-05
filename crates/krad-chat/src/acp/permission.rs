use serde::{Deserialize, Serialize};

pub use krad_domain::AcpPermissionKind as PermissionOptionKind;

use super::ids::{PermissionOptionId, SessionId};
use super::update::ToolCallUpdate;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionRequest {
    pub session_id: SessionId,
    pub tool_call: ToolCallUpdate,
    pub options: Vec<PermissionOption>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: PermissionOptionId,
    pub name: String,
    pub kind: PermissionOptionKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestPermissionResponse {
    pub outcome: RequestPermissionOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RequestPermissionOutcome {
    Cancelled,
    Selected {
        #[serde(rename = "optionId")]
        option_id: PermissionOptionId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn both_outcomes_are_written_as_the_spec_expects() {
        let selected = RequestPermissionResponse {
            outcome: RequestPermissionOutcome::Selected {
                option_id: PermissionOptionId("allow-once".to_owned()),
            },
        };
        let cancelled = RequestPermissionResponse {
            outcome: RequestPermissionOutcome::Cancelled,
        };
        assert_eq!(
            serde_json::to_value(&selected).ok(),
            Some(json!({"outcome": {"outcome": "selected", "optionId": "allow-once"}}))
        );
        assert_eq!(
            serde_json::to_value(&cancelled).ok(),
            Some(json!({"outcome": {"outcome": "cancelled"}}))
        );
    }
}
