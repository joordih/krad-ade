use krad_domain::{AgentPreset, ResumeId};
use serde::{Deserialize, Serialize};

use crate::bus::AgentRef;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum LaunchedAs {
    Shell,
    WslShell,
    Line,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TerminalLaunched {
    pub terminal: AgentRef,
    pub harness: Option<AgentPreset>,
    pub launched_as: LaunchedAs,
    #[serde(default)]
    pub resume: Option<ResumeId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq, TerminalCommand};
    use krad_domain::{EnvironmentName, LaunchIntent, TerminalId};

    fn terminal() -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("main".to_owned()),
            terminal: TerminalId(7),
        }
    }

    #[test]
    fn opening_a_terminal_and_its_launch_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Terminal(TerminalCommand::Open {
            terminal: terminal(),
            intent: LaunchIntent::Preset {
                preset: AgentPreset::Shell,
            },
            cwd: None,
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"terminal","verb":"open","terminal":{"environment":"main","terminal":7},"intent":{"intent":"preset","preset":"Shell"},"cwd":null}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let missing = Command::Terminal(TerminalCommand::OpenMissing {
            environment: EnvironmentName::new("main".to_owned()),
        });
        let json = serde_json::to_string(&missing)?;
        assert_eq!(
            json,
            r#"{"area":"terminal","verb":"open_missing","environment":"main"}"#
        );
        let event = Event {
            seq: Seq(5),
            kind: EventKind::TerminalLaunched(TerminalLaunched {
                terminal: terminal(),
                harness: None,
                launched_as: LaunchedAs::Shell,
                resume: None,
            }),
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        Ok(())
    }
}
