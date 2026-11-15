use krad_domain::EnvironmentName;
use serde::{Deserialize, Serialize};

use crate::bus::AgentRef;
use crate::command::ErrorCode;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Problem {
    pub scope: ProblemScope,
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "on", rename_all = "snake_case")]
pub enum ProblemScope {
    Runtime,
    Environment {
        environment: EnvironmentName,
    },
    Chat {
        chat: AgentRef,
    },
    CloudSession {
        slug: String,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::TerminalId;

    #[test]
    fn a_problem_names_its_scope_and_code_on_the_wire() -> serde_json::Result<()> {
        let problem = Problem {
            scope: ProblemScope::Chat {
                chat: AgentRef {
                    environment: EnvironmentName::new("main".to_owned()),
                    terminal: TerminalId(3),
                },
            },
            code: ErrorCode::Unavailable,
            message: "disk full".to_owned(),
        };
        let json = serde_json::to_value(&problem)?;
        assert_eq!(json["scope"]["on"], "chat");
        assert_eq!(json["code"], "unavailable");
        assert_eq!(serde_json::from_value::<Problem>(json)?, problem);
        let newer = serde_json::json!({ "on": "galaxy" });
        assert_eq!(
            serde_json::from_value::<ProblemScope>(newer)?,
            ProblemScope::Unknown
        );
        Ok(())
    }
}
