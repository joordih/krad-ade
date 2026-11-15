use krad_domain::{EnvironmentEdit, EnvironmentName, GroupEdit, NodeEdit, SessionEdit};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SessionCommand {
    pub environment: EnvironmentName,
    pub edit: SessionEdit,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroupCommand {
    pub environment: EnvironmentName,
    pub edit: GroupEdit,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NodeCommand {
    pub environment: EnvironmentName,
    pub edit: NodeEdit,
}

impl From<SessionCommand> for (EnvironmentName, EnvironmentEdit) {
    fn from(command: SessionCommand) -> Self {
        (command.environment, command.edit.into())
    }
}

impl From<GroupCommand> for (EnvironmentName, EnvironmentEdit) {
    fn from(command: GroupCommand) -> Self {
        (command.environment, command.edit.into())
    }
}

impl From<NodeCommand> for (EnvironmentName, EnvironmentEdit) {
    fn from(command: NodeCommand) -> Self {
        (command.environment, command.edit.into())
    }
}

impl From<(EnvironmentName, EnvironmentEdit)> for crate::Command {
    fn from((environment, edit): (EnvironmentName, EnvironmentEdit)) -> Self {
        match edit {
            EnvironmentEdit::Session(edit) => Self::Session(SessionCommand { environment, edit }),
            EnvironmentEdit::Group(edit) => Self::Group(GroupCommand { environment, edit }),
            EnvironmentEdit::Node(edit) => Self::Node(NodeCommand { environment, edit }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, RequestId, Seq};
    use krad_domain::{EnvironmentChange, GroupId, SessionId};

    fn main() -> EnvironmentName {
        EnvironmentName::new("main".to_owned())
    }

    #[test]
    fn session_commands_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Session(SessionCommand {
            environment: main(),
            edit: SessionEdit::Close {
                session: SessionId(4),
            },
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"session","environment":"main","edit":{"verb":"close","session":4}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let group = Command::Group(GroupCommand {
            environment: main(),
            edit: GroupEdit::Remove { group: GroupId(2) },
        });
        let json = serde_json::to_string(&group)?;
        assert_eq!(serde_json::from_str::<Command>(&json)?, group);
        let newer: Command = serde_json::from_str(
            r#"{"area":"node","environment":"main","edit":{"verb":"teleport","terminal":3}}"#,
        )?;
        assert_eq!(
            newer,
            Command::Node(NodeCommand {
                environment: main(),
                edit: NodeEdit::Unknown,
            })
        );
        Ok(())
    }

    #[test]
    fn a_change_tells_the_request_it_answers_and_older_ones_answer_none() -> serde_json::Result<()>
    {
        let event = Event {
            seq: Seq(3),
            kind: EventKind::EnvironmentChanged {
                environment: main(),
                changes: vec![EnvironmentChange::SessionRemoved { id: SessionId(4) }],
                answers: Some(RequestId(9)),
            },
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        let older: EventKind = serde_json::from_str(
            r#"{"type":"environment_changed","environment":"main","changes":[]}"#,
        )?;
        assert_eq!(
            older,
            EventKind::EnvironmentChanged {
                environment: main(),
                changes: Vec::new(),
                answers: None,
            }
        );
        Ok(())
    }
}
