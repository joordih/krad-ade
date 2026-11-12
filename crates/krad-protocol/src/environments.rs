use krad_domain::{
    BudgetStatus, EnvironmentChange, EnvironmentData, EnvironmentName, GroupId, SpendSettings,
    TerminalId, UsageReport, UsdCost,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum EnvironmentCommand {
    Open {
        environment: Box<EnvironmentData>,
    },
    Close {
        environment: EnvironmentName,
    },
    Order {
        order: Vec<EnvironmentName>,
    },
    Change {
        environment: EnvironmentName,
        changes: Vec<EnvironmentChange>,
    },
    RecordUsage {
        environment: EnvironmentName,
        chat: TerminalId,
        report: UsageReport,
        spending: SpendSettings,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SpendingCrossed {
    pub environment: EnvironmentName,
    pub group: Option<GroupId>,
    pub status: BudgetStatus,
    pub spent: UsdCost,
    pub budget: UsdCost,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};

    fn main() -> EnvironmentName {
        EnvironmentName::new("main".to_owned())
    }

    #[test]
    fn a_change_command_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Environments(EnvironmentCommand::Change {
            environment: main(),
            changes: vec![EnvironmentChange::NextId { next: 5 }],
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"environments","verb":"change","environment":"main","changes":[{"change":"next_id","next":5}]}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"environments","verb":"rename"}"#)?;
        assert_eq!(newer, Command::Environments(EnvironmentCommand::Unknown));
        Ok(())
    }

    #[test]
    fn environment_events_round_trip_without_the_view() -> serde_json::Result<()> {
        let mut opened = EnvironmentData::empty(main());
        opened.focus = Some(TerminalId(3));
        let events = [
            EventKind::EnvironmentOpened {
                environment: Box::new(EnvironmentData::empty(main())),
            },
            EventKind::EnvironmentChanged {
                environment: main(),
                changes: vec![EnvironmentChange::NodeRemoved { id: TerminalId(2) }],
            },
            EventKind::EnvironmentsOrdered {
                order: vec![main()],
            },
            EventKind::EnvironmentClosed {
                environment: main(),
            },
            EventKind::SpendingCrossed(SpendingCrossed {
                environment: main(),
                group: Some(GroupId(1)),
                status: BudgetStatus::Exceeded,
                spent: UsdCost::from_nanos(2),
                budget: UsdCost::from_nanos(1),
            }),
        ];
        for kind in events {
            let event = Event { seq: Seq(1), kind };
            let json = serde_json::to_string(&event)?;
            assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        }
        let json = serde_json::to_string(&EventKind::EnvironmentOpened {
            environment: Box::new(opened),
        })?;
        assert!(!json.contains("focus"), "{json}");
        Ok(())
    }
}
