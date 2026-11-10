use super::automation::{AppTrigger, AutomationEvent, CodeHostWake};
use crate::event::EventKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KraddieEvent {
    AgentFinished,
    CiFailed,
    MergeRequestOpened,
    GitChanged,
    WorktreeCreated,
}

impl KraddieEvent {
    #[must_use]
    pub const fn label(self) -> &'static str {
        EventKind::from_kraddie(self).label()
    }
}

impl EventKind {
    const fn from_kraddie(event: KraddieEvent) -> Self {
        match event {
            KraddieEvent::AgentFinished => Self::AgentFinished,
            KraddieEvent::CiFailed => Self::CiFailed,
            KraddieEvent::MergeRequestOpened => Self::MergeRequestOpened,
            KraddieEvent::GitChanged => Self::GitChanged,
            KraddieEvent::WorktreeCreated => Self::WorktreeCreated,
        }
    }
}

impl From<KraddieEvent> for EventKind {
    fn from(event: KraddieEvent) -> Self {
        Self::from_kraddie(event)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "event", rename_all = "snake_case")]
pub enum KraddieWake {
    Schedule,
    CatchUp,
    Event(KraddieEvent),
    App(AppTrigger),
    CodeHost(CodeHostWake),
    Manual,
    Cli,
    Rounds,
}

impl KraddieWake {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Schedule => "Schedule",
            Self::CatchUp => "Missed while the app was closed",
            Self::Event(event) => event.label(),
            Self::App(trigger) => trigger.label(),
            Self::CodeHost(wake) => wake.label(),
            Self::Manual => "Run now",
            Self::Cli => "krad CLI",
            Self::Rounds => "Check-in",
        }
    }

    #[must_use]
    pub const fn by_user(self) -> bool {
        matches!(self, Self::Manual | Self::Cli)
    }
}

impl From<AutomationEvent> for KraddieWake {
    fn from(event: AutomationEvent) -> Self {
        match event {
            AutomationEvent::App(trigger) => Self::App(trigger),
            AutomationEvent::CodeHost(wake) => Self::CodeHost(wake),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::automation::CodeHostTrigger;
    use super::*;
    use crate::code_host::CodeHost;

    #[test]
    fn wakes_keep_their_shape_on_disk_and_old_event_wakes_still_read() {
        let old: KraddieWake = serde_json::from_str(r#"{"kind":"event","event":"ci_failed"}"#)
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(old, KraddieWake::Event(KraddieEvent::CiFailed));
        assert_eq!(old.label(), "CI failed");
        assert_eq!(
            serde_json::from_str::<KraddieWake>(r#"{"kind":"cli"}"#)
                .unwrap_or_else(|_| unreachable!()),
            KraddieWake::Cli
        );
        let wake = KraddieWake::from(AutomationEvent::CodeHost(CodeHostWake {
            host: CodeHost::Github,
            event: CodeHostTrigger::RequestOpened,
        }));
        let line = serde_json::to_string(&wake).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            line,
            r#"{"kind":"code_host","event":{"host":"Github","event":"request_opened"}}"#
        );
        assert_eq!(wake.label(), "Pull request opened");
        assert!(KraddieWake::Cli.by_user());
        assert!(!KraddieWake::CatchUp.by_user());
    }
}
