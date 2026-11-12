use krad_domain::{AgentState, GridSize, ProcessRun};
use serde::{Deserialize, Serialize};

use crate::bus::AgentRef;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum TerminalCommand {
    Write {
        terminal: AgentRef,
        bytes: Vec<u8>,
        by: WriteBy,
    },
    Resize {
        terminal: AgentRef,
        size: GridSize,
    },
    Scroll {
        terminal: AgentRef,
        scroll: TerminalScroll,
    },
    Close {
        terminal: AgentRef,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteBy {
    User,
    App,
    AppOnUntouchedPrompt,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum TerminalScroll {
    Lines { lines: i32 },
    Bottom,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TerminalSummary {
    pub terminal: AgentRef,
    pub run: ProcessRun,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TerminalClassified {
    pub terminal: AgentRef,
    pub state: AgentState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::event::{Event, EventKind};
    use crate::ids::Seq;
    use krad_domain::{EnvironmentName, TerminalId};

    fn terminal() -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("main".to_owned()),
            terminal: TerminalId(4),
        }
    }

    #[test]
    fn a_terminal_write_has_its_wire_shape() -> serde_json::Result<()> {
        let command = Command::Terminal(TerminalCommand::Write {
            terminal: terminal(),
            bytes: b"ls\r".to_vec(),
            by: WriteBy::User,
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"terminal","verb":"write","terminal":{"environment":"main","terminal":4},"bytes":[108,115,13],"by":"user"}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"terminal","verb":"split"}"#)?;
        assert_eq!(newer, Command::Terminal(TerminalCommand::Unknown));
        Ok(())
    }

    #[test]
    fn terminal_commands_and_events_round_trip() -> serde_json::Result<()> {
        let commands = [
            TerminalCommand::Resize {
                terminal: terminal(),
                size: GridSize {
                    cols: 100,
                    rows: 30,
                    cell_width_px: 8,
                    cell_height_px: 18,
                },
            },
            TerminalCommand::Scroll {
                terminal: terminal(),
                scroll: TerminalScroll::Lines { lines: -3 },
            },
            TerminalCommand::Scroll {
                terminal: terminal(),
                scroll: TerminalScroll::Bottom,
            },
            TerminalCommand::Close {
                terminal: terminal(),
            },
        ];
        for command in commands {
            let json = serde_json::to_string(&command)?;
            assert_eq!(serde_json::from_str::<TerminalCommand>(&json)?, command);
        }
        let events = [
            EventKind::TerminalChanged {
                terminal: terminal(),
            },
            EventKind::TerminalEnded(TerminalSummary {
                terminal: terminal(),
                run: ProcessRun::Exited { code: Some(2) },
            }),
            EventKind::TerminalClassified(TerminalClassified {
                terminal: terminal(),
                state: AgentState::Working,
            }),
        ];
        for kind in events {
            let event = Event { seq: Seq(1), kind };
            let json = serde_json::to_string(&event)?;
            assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        }
        Ok(())
    }
}
