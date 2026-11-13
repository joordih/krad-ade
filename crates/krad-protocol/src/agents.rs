use krad_domain::{AgentPreset, EnvironmentName, SessionId, TerminalId, WhenWorking, WorktreeFate};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AgentCommand {
    pub environment: EnvironmentName,
    pub lead: SessionId,
    pub act: AgentAct,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum AgentAct {
    StartWorker {
        agent: AgentPreset,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worktree: Option<String>,
        task: String,
    },
    CloseWorker {
        worker: TerminalId,
        #[serde(default)]
        when_working: WhenWorking,
        #[serde(default)]
        worktree: WorktreeFate,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    #[test]
    fn agent_commands_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let close = Command::Agent(AgentCommand {
            environment: EnvironmentName::new("main".to_owned()),
            lead: SessionId(2),
            act: AgentAct::CloseWorker {
                worker: TerminalId(7),
                when_working: WhenWorking::CancelTurn,
                worktree: WorktreeFate::Remove,
            },
        });
        let json = serde_json::to_string(&close)?;
        assert_eq!(
            json,
            r#"{"area":"agent","environment":"main","lead":2,"act":{"verb":"close_worker","worker":7,"when_working":"cancel_turn","worktree":"remove"}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, close);
        let older: AgentAct = serde_json::from_str(r#"{"verb":"close_worker","worker":7}"#)?;
        assert_eq!(
            older,
            AgentAct::CloseWorker {
                worker: TerminalId(7),
                when_working: WhenWorking::Refuse,
                worktree: WorktreeFate::Keep,
            }
        );
        let newer: AgentAct = serde_json::from_str(r#"{"verb":"pause_worker","worker":7}"#)?;
        assert_eq!(newer, AgentAct::Unknown);
        Ok(())
    }
}
