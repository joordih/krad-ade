use super::super::schedule::KraddieSchedule;
use crate::code_host::CodeHost;
use crate::event::{AppEvent, EventKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppTrigger {
    AgentFinished,
    AgentWaiting,
    GitChanged,
    WorktreeCreated,
    WorktreeRemoved,
    VariantsFinished,
}

impl AppTrigger {
    pub const ALL: [Self; 6] = [
        Self::AgentFinished,
        Self::AgentWaiting,
        Self::GitChanged,
        Self::WorktreeCreated,
        Self::WorktreeRemoved,
        Self::VariantsFinished,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        EventKind::from_app_trigger(self).label()
    }

    #[must_use]
    pub const fn of(event: &AppEvent) -> Option<Self> {
        match event {
            AppEvent::AgentFinished { .. } => Some(Self::AgentFinished),
            AppEvent::AgentWaiting { .. } => Some(Self::AgentWaiting),
            AppEvent::GitChanged => Some(Self::GitChanged),
            AppEvent::WorktreeCreated { .. } => Some(Self::WorktreeCreated),
            AppEvent::WorktreeRemoved => Some(Self::WorktreeRemoved),
            AppEvent::VariantsFinished { .. } => Some(Self::VariantsFinished),
            AppEvent::AgentStarted { .. }
            | AppEvent::SessionOpened { .. }
            | AppEvent::SessionClosed { .. }
            | AppEvent::ReviewSent { .. }
            | AppEvent::CiFailed { .. }
            | AppEvent::CiPassed { .. }
            | AppEvent::MergeRequestOpened { .. }
            | AppEvent::VariantsStarted { .. }
            | AppEvent::VariantWinnerChosen { .. }
            | AppEvent::Ping => None,
        }
    }
}

impl EventKind {
    const fn from_app_trigger(trigger: AppTrigger) -> Self {
        match trigger {
            AppTrigger::AgentFinished => Self::AgentFinished,
            AppTrigger::AgentWaiting => Self::AgentWaiting,
            AppTrigger::GitChanged => Self::GitChanged,
            AppTrigger::WorktreeCreated => Self::WorktreeCreated,
            AppTrigger::WorktreeRemoved => Self::WorktreeRemoved,
            AppTrigger::VariantsFinished => Self::VariantsFinished,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeHostTrigger {
    RequestOpened,
    CiFailed,
    CiPassed,
}

impl CodeHostTrigger {
    pub const ALL: [Self; 3] = [Self::RequestOpened, Self::CiFailed, Self::CiPassed];

    #[must_use]
    pub const fn label(self, host: CodeHost) -> &'static str {
        match (host, self) {
            (CodeHost::Github, Self::RequestOpened) => "Pull request opened",
            (CodeHost::Gitlab, Self::RequestOpened) => "Merge request opened",
            (CodeHost::Github, Self::CiFailed) => "CI failed",
            (CodeHost::Gitlab, Self::CiFailed) => "Pipeline failed",
            (CodeHost::Github, Self::CiPassed) => "CI passed",
            (CodeHost::Gitlab, Self::CiPassed) => "Pipeline passed",
        }
    }

    #[must_use]
    pub const fn of(event: &AppEvent) -> Option<Self> {
        match event {
            AppEvent::MergeRequestOpened { .. } => Some(Self::RequestOpened),
            AppEvent::CiFailed { .. } => Some(Self::CiFailed),
            AppEvent::CiPassed { .. } => Some(Self::CiPassed),
            _ => None,
        }
    }
}

#[must_use]
pub const fn code_host_name(host: CodeHost) -> &'static str {
    match host {
        CodeHost::Github => "GitHub",
        CodeHost::Gitlab => "GitLab",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CodeHostWake {
    pub host: CodeHost,
    pub event: CodeHostTrigger,
}

impl CodeHostWake {
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.event.label(self.host)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AutomationEvent {
    App(AppTrigger),
    CodeHost(CodeHostWake),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    Schedule {
        schedule: KraddieSchedule,
    },
    App {
        event: AppTrigger,
    },
    CodeHost {
        host: CodeHost,
        event: CodeHostTrigger,
    },
    Manual,
}

impl Trigger {
    #[must_use]
    pub fn hears(self, event: AutomationEvent) -> bool {
        match (self, event) {
            (Self::App { event: wanted }, AutomationEvent::App(heard)) => wanted == heard,
            (
                Self::CodeHost {
                    host,
                    event: wanted,
                },
                AutomationEvent::CodeHost(heard),
            ) => host == heard.host && wanted == heard.event,
            _ => false,
        }
    }

    #[must_use]
    pub const fn schedule(&self) -> Option<&KraddieSchedule> {
        match self {
            Self::Schedule { schedule } => Some(schedule),
            _ => None,
        }
    }

    #[must_use]
    pub const fn code_host(&self) -> Option<CodeHost> {
        match self {
            Self::CodeHost { host, .. } => Some(*host),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TerminalId;
    use crate::review::{MergeRequestIid, PipelineId};

    #[test]
    fn bus_events_map_to_app_or_code_host_triggers_but_never_both() {
        let finished = AppEvent::AgentFinished {
            terminal: TerminalId(2),
        };
        assert_eq!(AppTrigger::of(&finished), Some(AppTrigger::AgentFinished));
        assert_eq!(CodeHostTrigger::of(&finished), None);
        let failed = AppEvent::CiFailed {
            merge_request: MergeRequestIid(4),
            pipeline: PipelineId(9),
        };
        assert_eq!(AppTrigger::of(&failed), None);
        assert_eq!(
            CodeHostTrigger::of(&failed),
            Some(CodeHostTrigger::CiFailed)
        );
        assert_eq!(AppTrigger::of(&AppEvent::Ping), None);
        for trigger in AppTrigger::ALL {
            assert_eq!(
                trigger.label(),
                EventKind::from_app_trigger(trigger).label()
            );
        }
    }

    #[test]
    fn a_trigger_hears_only_its_own_event_on_its_own_host() {
        let github_failed = AutomationEvent::CodeHost(CodeHostWake {
            host: CodeHost::Github,
            event: CodeHostTrigger::CiFailed,
        });
        let gitlab_failed = AutomationEvent::CodeHost(CodeHostWake {
            host: CodeHost::Gitlab,
            event: CodeHostTrigger::CiFailed,
        });
        let trigger = Trigger::CodeHost {
            host: CodeHost::Github,
            event: CodeHostTrigger::CiFailed,
        };
        assert!(trigger.hears(github_failed));
        assert!(!trigger.hears(gitlab_failed));
        assert!(!Trigger::Manual.hears(github_failed));
        let git = Trigger::App {
            event: AppTrigger::GitChanged,
        };
        assert!(git.hears(AutomationEvent::App(AppTrigger::GitChanged)));
        assert!(!git.hears(AutomationEvent::App(AppTrigger::AgentFinished)));
        assert_eq!(
            CodeHostTrigger::RequestOpened.label(CodeHost::Github),
            "Pull request opened"
        );
        assert_eq!(
            CodeHostTrigger::RequestOpened.label(CodeHost::Gitlab),
            "Merge request opened"
        );
    }

    #[test]
    fn triggers_keep_their_shape_on_disk() {
        let line = serde_json::to_string(&Trigger::CodeHost {
            host: CodeHost::Gitlab,
            event: CodeHostTrigger::CiFailed,
        })
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            line,
            r#"{"kind":"code_host","host":"Gitlab","event":"ci_failed"}"#
        );
        let manual: Trigger =
            serde_json::from_str(r#"{"kind":"manual"}"#).unwrap_or_else(|_| unreachable!());
        assert_eq!(manual, Trigger::Manual);
        let every: Trigger =
            serde_json::from_str(r#"{"kind":"schedule","schedule":{"kind":"every","minutes":30}}"#)
                .unwrap_or_else(|_| unreachable!());
        assert!(every.schedule().is_some());
    }
}
