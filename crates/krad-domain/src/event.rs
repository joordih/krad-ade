mod code_host_watch;
mod delivery;
mod timestamp;
mod webhook;

pub use code_host_watch::{code_host_events, CodeHostSnapshot, PipelineWatch};
pub use delivery::{
    backoff_after, next_delivery_step, DeliveryOutcome, DeliveryStep, WEBHOOK_FIRST_BACKOFF,
    WEBHOOK_MAX_ATTEMPTS, WEBHOOK_MAX_BACKOFF,
};
pub use timestamp::Timestamp;
pub use webhook::{
    Webhook, WebhookId, WebhookName, WebhookNameError, WebhookSecret, WebhookSettings,
    WebhookSigning, WebhookUrl,
};

use crate::ids::{EnvironmentName, GroupId, SessionId, TerminalId};
use crate::review::{MergeRequestIid, PipelineId};
use crate::variants::{VariantId, VariantRunId, WinnerOutcome};
use crate::AgentPreset;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventApiVersion(pub u32);

pub const EVENT_API_VERSION: EventApiVersion = EventApiVersion(1);

impl EventApiVersion {
    #[must_use]
    pub const fn is_readable(self) -> bool {
        self.0 <= EVENT_API_VERSION.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    AgentStarted,
    AgentWaiting,
    AgentFinished,
    WorktreeCreated,
    WorktreeRemoved,
    SessionOpened,
    SessionClosed,
    GitChanged,
    ReviewSent,
    CiFailed,
    CiPassed,
    MergeRequestOpened,
    VariantsStarted,
    VariantsFinished,
    VariantWinnerChosen,
    Ping,
}

impl EventKind {
    pub const SELECTABLE: [Self; 15] = [
        Self::AgentStarted,
        Self::AgentWaiting,
        Self::AgentFinished,
        Self::WorktreeCreated,
        Self::WorktreeRemoved,
        Self::SessionOpened,
        Self::SessionClosed,
        Self::GitChanged,
        Self::ReviewSent,
        Self::CiFailed,
        Self::CiPassed,
        Self::MergeRequestOpened,
        Self::VariantsStarted,
        Self::VariantsFinished,
        Self::VariantWinnerChosen,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AgentStarted => "Agent started",
            Self::AgentWaiting => "Agent waits for input",
            Self::AgentFinished => "Agent finished",
            Self::WorktreeCreated => "Worktree created",
            Self::WorktreeRemoved => "Worktree removed",
            Self::SessionOpened => "Session opened",
            Self::SessionClosed => "Session closed",
            Self::GitChanged => "Git changes",
            Self::ReviewSent => "Review sent",
            Self::CiFailed => "CI failed",
            Self::CiPassed => "CI passed",
            Self::MergeRequestOpened => "Merge request opened",
            Self::VariantsStarted => "Variants started",
            Self::VariantsFinished => "Variants finished",
            Self::VariantWinnerChosen => "Variant winner chosen",
            Self::Ping => "Test event",
        }
    }

    #[must_use]
    pub const fn comes_from_code_host(self) -> bool {
        matches!(
            self,
            Self::CiFailed | Self::CiPassed | Self::MergeRequestOpened
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Commented,
    Replied,
    Approved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppEvent {
    AgentStarted {
        terminal: TerminalId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        harness: Option<AgentPreset>,
    },
    AgentWaiting {
        terminal: TerminalId,
    },
    AgentFinished {
        terminal: TerminalId,
    },
    WorktreeCreated {
        from: GroupId,
    },
    WorktreeRemoved,
    SessionOpened {
        session: SessionId,
    },
    SessionClosed {
        session: SessionId,
    },
    GitChanged,
    ReviewSent {
        merge_request: MergeRequestIid,
        verdict: ReviewVerdict,
    },
    CiFailed {
        merge_request: MergeRequestIid,
        pipeline: PipelineId,
    },
    CiPassed {
        merge_request: MergeRequestIid,
        pipeline: PipelineId,
    },
    MergeRequestOpened {
        merge_request: MergeRequestIid,
    },
    VariantsStarted {
        run: VariantRunId,
        variants: usize,
    },
    VariantsFinished {
        run: VariantRunId,
    },
    VariantWinnerChosen {
        run: VariantRunId,
        variant: VariantId,
        outcome: WinnerOutcome,
    },
    Ping,
}

impl AppEvent {
    #[must_use]
    pub const fn kind(&self) -> EventKind {
        match self {
            Self::AgentStarted { .. } => EventKind::AgentStarted,
            Self::AgentWaiting { .. } => EventKind::AgentWaiting,
            Self::AgentFinished { .. } => EventKind::AgentFinished,
            Self::WorktreeCreated { .. } => EventKind::WorktreeCreated,
            Self::WorktreeRemoved => EventKind::WorktreeRemoved,
            Self::SessionOpened { .. } => EventKind::SessionOpened,
            Self::SessionClosed { .. } => EventKind::SessionClosed,
            Self::GitChanged => EventKind::GitChanged,
            Self::ReviewSent { .. } => EventKind::ReviewSent,
            Self::CiFailed { .. } => EventKind::CiFailed,
            Self::CiPassed { .. } => EventKind::CiPassed,
            Self::MergeRequestOpened { .. } => EventKind::MergeRequestOpened,
            Self::VariantsStarted { .. } => EventKind::VariantsStarted,
            Self::VariantsFinished { .. } => EventKind::VariantsFinished,
            Self::VariantWinnerChosen { .. } => EventKind::VariantWinnerChosen,
            Self::Ping => EventKind::Ping,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    pub api: EventApiVersion,
    pub at: Timestamp,
    pub environment: EnvironmentName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupId>,
    pub event: AppEvent,
}

impl EventRecord {
    #[must_use]
    pub fn new(
        at: SystemTime,
        environment: EnvironmentName,
        group: Option<GroupId>,
        event: AppEvent,
    ) -> Self {
        Self {
            api: EVENT_API_VERSION,
            at: Timestamp::from(at),
            environment,
            group,
            event,
        }
    }

    #[must_use]
    pub fn read(line: &str) -> Option<Self> {
        let record: Self = serde_json::from_str(line).ok()?;
        record.api.is_readable().then_some(record)
    }
}

impl TryFrom<&EventRecord> for String {
    type Error = serde_json::Error;

    fn try_from(record: &EventRecord) -> Result<Self, Self::Error> {
        serde_json::to_string(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::time::{Duration, UNIX_EPOCH};

    const AN_HOUR: Duration = Duration::from_secs(3600);

    fn record(event: AppEvent) -> EventRecord {
        EventRecord::new(
            UNIX_EPOCH + AN_HOUR,
            EnvironmentName::new("main".to_owned()),
            Some(GroupId(2)),
            event,
        )
    }

    #[test]
    fn events_serialize_as_tagged_ids_with_the_api_version() -> serde_json::Result<()> {
        let written = serde_json::to_value(record(AppEvent::AgentStarted {
            terminal: TerminalId(7),
            harness: Some(AgentPreset::ClaudeCode),
        }))?;
        assert_eq!(written["api"], json!(EVENT_API_VERSION.0));
        assert_eq!(written["environment"], "main");
        assert_eq!(written["group"], 2);
        assert_eq!(written["event"]["kind"], "agent_started");
        assert_eq!(written["event"]["terminal"], 7);
        let unit = serde_json::to_value(record(AppEvent::GitChanged))?;
        assert_eq!(unit["event"], json!({ "kind": "git_changed" }));
        Ok(())
    }

    #[test]
    fn every_event_reads_back_as_itself() -> serde_json::Result<()> {
        let events = [
            AppEvent::AgentWaiting {
                terminal: TerminalId(1),
            },
            AppEvent::WorktreeCreated { from: GroupId(1) },
            AppEvent::WorktreeRemoved,
            AppEvent::SessionClosed {
                session: SessionId(4),
            },
            AppEvent::ReviewSent {
                merge_request: MergeRequestIid(12),
                verdict: ReviewVerdict::Approved,
            },
            AppEvent::CiFailed {
                merge_request: MergeRequestIid(12),
                pipeline: PipelineId(99),
            },
            AppEvent::VariantsStarted {
                run: VariantRunId(3),
                variants: 2,
            },
            AppEvent::VariantsFinished {
                run: VariantRunId(3),
            },
            AppEvent::VariantWinnerChosen {
                run: VariantRunId(3),
                variant: VariantId(2),
                outcome: WinnerOutcome::ApplyOntoBase,
            },
            AppEvent::Ping,
        ];
        for event in events {
            let line = String::try_from(&record(event.clone()))?;
            assert_eq!(EventRecord::read(&line), Some(record(event)));
        }
        Ok(())
    }

    #[test]
    fn the_record_time_is_an_rfc_3339_utc_timestamp_and_the_old_form_is_not_read(
    ) -> serde_json::Result<()> {
        let written = serde_json::to_value(record(AppEvent::GitChanged))?;
        assert_eq!(written["at"], json!("1970-01-01T01:00:00.000Z"));
        let mut old: Value = written;
        old["at"] = json!({ "secs_since_epoch": 3600, "nanos_since_epoch": 0 });
        assert_eq!(EventRecord::read(&old.to_string()), None);
        Ok(())
    }

    #[test]
    fn records_of_a_newer_api_or_an_unknown_kind_are_not_read() -> serde_json::Result<()> {
        let mut newer: Value = serde_json::to_value(record(AppEvent::GitChanged))?;
        newer["api"] = json!(EVENT_API_VERSION.0 + 1);
        assert_eq!(EventRecord::read(&newer.to_string()), None);
        let mut unknown: Value = serde_json::to_value(record(AppEvent::GitChanged))?;
        unknown["event"] = json!({ "kind": "galaxy_collapsed" });
        assert_eq!(EventRecord::read(&unknown.to_string()), None);
        assert_eq!(EventRecord::read("{ not json"), None);
        Ok(())
    }

    #[test]
    fn each_event_names_its_kind_and_only_code_host_kinds_need_polling() {
        let kind = AppEvent::CiPassed {
            merge_request: MergeRequestIid(1),
            pipeline: PipelineId(2),
        }
        .kind();
        assert_eq!(kind, EventKind::CiPassed);
        assert!(kind.comes_from_code_host());
        assert!(!EventKind::GitChanged.comes_from_code_host());
        assert!(!EventKind::SELECTABLE.contains(&EventKind::Ping));
    }
}
