use krad_domain::{AgentPreset, HarnessHealth, Host};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HealthPlace {
    ThisComputer,
    Wsl { distro: String },
    Ssh { host: Host },
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "state", content = "health", rename_all = "snake_case")]
pub enum HarnessCheck {
    Checking,
    Done(HarnessHealth),
    Forgotten,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HarnessChecked {
    pub place: HealthPlace,
    pub harness: AgentPreset,
    pub check: HarnessCheck,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct HealthSnapshot {
    #[serde(default)]
    pub checks: Vec<HarnessChecked>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, EventKind, Seq};
    use krad_domain::{HarnessVersion, SignIn};

    #[test]
    fn a_harness_check_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let event = Event {
            seq: Seq(3),
            kind: EventKind::HarnessChecked(HarnessChecked {
                place: HealthPlace::Wsl {
                    distro: "Ubuntu".to_owned(),
                },
                harness: AgentPreset::Codex,
                check: HarnessCheck::Checking,
            }),
        };
        let json = serde_json::to_string(&event)?;
        assert_eq!(
            json,
            r#"{"seq":3,"kind":{"type":"harness_checked","place":{"kind":"wsl","distro":"Ubuntu"},"harness":"Codex","check":{"state":"checking"}}}"#
        );
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        Ok(())
    }

    #[test]
    fn a_finished_check_round_trips_with_its_health() -> serde_json::Result<()> {
        let checked = HarnessChecked {
            place: HealthPlace::ThisComputer,
            harness: AgentPreset::ClaudeCode,
            check: HarnessCheck::Done(HarnessHealth::Found {
                version: Some(HarnessVersion::new(2, 1, 0)),
                sign_in: SignIn::SignedIn,
            }),
        };
        let json = serde_json::to_string(&checked)?;
        assert_eq!(serde_json::from_str::<HarnessChecked>(&json)?, checked);
        Ok(())
    }
}
