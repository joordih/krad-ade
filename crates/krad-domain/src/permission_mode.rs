use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    #[serde(alias = "default")]
    Manual,
    AcceptEdits,
    Plan,
    Auto,
    DontAsk,
    BypassPermissions,
}

impl fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Manual => "manual",
            Self::AcceptEdits => "acceptEdits",
            Self::Plan => "plan",
            Self::Auto => "auto",
            Self::DontAsk => "dontAsk",
            Self::BypassPermissions => "bypassPermissions",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PermissionMode;
    use serde_json::{json, Value};

    #[test]
    fn the_flag_name_matches_the_protocol_name() {
        for mode in [
            PermissionMode::Manual,
            PermissionMode::AcceptEdits,
            PermissionMode::Plan,
            PermissionMode::Auto,
            PermissionMode::DontAsk,
            PermissionMode::BypassPermissions,
        ] {
            assert_eq!(json!(mode), Value::from(mode.to_string()));
        }
    }
}
