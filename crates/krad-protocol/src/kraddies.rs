use krad_domain::{
    Automation, AutomationId, AutomationKey, DigestBook, DigestDraft, DigestId, EnvironmentName,
    Kraddie, KraddieActivity, KraddieCompass, KraddieHistory, KraddieId, KraddieMind, MemoryLine,
    MemoryOwner, Switch,
};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AutomationState {
    pub automation: Automation,
    #[serde(default)]
    pub history: KraddieHistory,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KraddieState {
    pub kraddie: Kraddie,
    #[serde(default)]
    pub automations: Vec<AutomationState>,
    #[serde(default)]
    pub check_ins: KraddieHistory,
    #[serde(default)]
    pub mind: KraddieMind,
    #[serde(default)]
    pub activity: KraddieActivity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check_in: Option<SystemTime>,
}

impl KraddieState {
    #[must_use]
    pub fn new(kraddie: Kraddie) -> Self {
        Self {
            kraddie,
            automations: Vec::new(),
            check_ins: KraddieHistory::default(),
            mind: KraddieMind::default(),
            activity: KraddieActivity::default(),
            last_check_in: None,
        }
    }

    #[must_use]
    pub fn id(&self) -> KraddieId {
        self.kraddie.id()
    }

    #[must_use]
    pub fn automation(&self, id: AutomationId) -> Option<&AutomationState> {
        self.automations
            .iter()
            .find(|state| state.automation.id == id)
    }

    pub fn automation_mut(&mut self, id: AutomationId) -> Option<&mut AutomationState> {
        self.automations
            .iter_mut()
            .find(|state| state.automation.id == id)
    }

    #[must_use]
    pub fn default_automation(&self) -> Option<&AutomationState> {
        self.automations
            .iter()
            .find(|state| state.automation.is_manual())
            .or_else(|| self.automations.first())
    }

    #[must_use]
    pub fn last_started(&self) -> Option<SystemTime> {
        self.automations
            .iter()
            .filter_map(|state| state.history.last_started())
            .chain(self.check_ins.last_started())
            .max()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OwnedDigests {
    pub owner: MemoryOwner,
    pub book: DigestBook,
}

#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct KraddiesSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened_at: Option<SystemTime>,
    #[serde(default)]
    pub kraddies: Vec<KraddieState>,
    #[serde(default)]
    pub digests: Vec<OwnedDigests>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum KraddieCommand {
    Save {
        kraddie: Box<Kraddie>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replaced: Option<KraddieId>,
    },
    Import {
        kraddie: Box<Kraddie>,
        automations: Vec<Automation>,
    },
    Delete {
        kraddie: KraddieId,
    },
    SaveAutomation {
        kraddie: KraddieId,
        automation: Box<Automation>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous: Option<AutomationKey>,
    },
    DeleteAutomation {
        automation: AutomationKey,
    },
    SwitchAutomation {
        automation: AutomationKey,
        switch: Switch,
    },
    Run {
        automation: AutomationKey,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    RunKraddie {
        kraddie: KraddieId,
    },
    OpenChat {
        kraddie: KraddieId,
        environment: EnvironmentName,
    },
    SetCompass {
        kraddie: KraddieId,
        compass: KraddieCompass,
    },
    Remember {
        kraddie: KraddieId,
        text: String,
    },
    ReviseMemory {
        kraddie: KraddieId,
        line: MemoryLine,
        text: String,
    },
    ForgetMemory {
        kraddie: KraddieId,
        line: MemoryLine,
    },
    ResetConversation {
        kraddie: KraddieId,
    },
    ReviseDigest {
        owner: MemoryOwner,
        digest: DigestId,
        text: String,
    },
    ForgetDigest {
        owner: MemoryOwner,
        digest: DigestId,
    },
    RecordDigest {
        owner: MemoryOwner,
        draft: Box<DigestDraft>,
    },
    UseNotes {
        titles: Vec<String>,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};
    use krad_domain::AgentPreset;

    fn kraddie() -> Kraddie {
        Kraddie::new(
            "Night Owl".parse().unwrap_or_else(|_| unreachable!()),
            AgentPreset::ClaudeCode,
        )
    }

    #[test]
    fn kraddie_commands_have_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Kraddie(KraddieCommand::RunKraddie {
            kraddie: kraddie().id(),
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"kraddie","verb":"run_kraddie","kraddie":"night-owl"}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let revise = Command::Kraddie(KraddieCommand::ReviseMemory {
            kraddie: kraddie().id(),
            line: MemoryLine {
                index: 0,
                text: "old".to_owned(),
            },
            text: "new".to_owned(),
        });
        let json = serde_json::to_string(&revise)?;
        assert_eq!(
            json,
            r#"{"area":"kraddie","verb":"revise_memory","kraddie":"night-owl","line":{"index":0,"text":"old"},"text":"new"}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, revise);
        let newer: Command = serde_json::from_str(r#"{"area":"kraddie","verb":"hatch"}"#)?;
        assert_eq!(newer, Command::Kraddie(KraddieCommand::Unknown));
        Ok(())
    }

    #[test]
    fn a_kraddie_and_its_digests_round_trip_as_events_and_snapshots() -> serde_json::Result<()> {
        let state = KraddieState::new(kraddie());
        let event = Event {
            seq: Seq(2),
            kind: EventKind::KraddieChanged {
                kraddie: Box::new(state.clone()),
            },
        };
        let json = serde_json::to_string(&event)?;
        assert!(json.starts_with(r#"{"seq":2,"kind":{"type":"kraddie_changed","kraddie":{"#));
        assert_eq!(serde_json::from_str::<Event>(&json)?, event);
        let snapshot = KraddiesSnapshot {
            opened_at: Some(SystemTime::UNIX_EPOCH),
            kraddies: vec![state],
            digests: vec![OwnedDigests {
                owner: MemoryOwner::Ordinary,
                book: DigestBook::default(),
            }],
        };
        let json = serde_json::to_string(&snapshot)?;
        assert_eq!(serde_json::from_str::<KraddiesSnapshot>(&json)?, snapshot);
        let older: KraddiesSnapshot = serde_json::from_str("{}")?;
        assert_eq!(older, KraddiesSnapshot::default());
        Ok(())
    }
}
