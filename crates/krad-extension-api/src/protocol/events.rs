use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::protocol::{notification, request, Empty};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AppEventKind {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Commented,
    Replied,
    Approved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppEvent {
    AgentStarted {
        terminal: String,
        #[serde(default)]
        harness: Option<String>,
    },
    AgentWaiting {
        terminal: String,
    },
    AgentFinished {
        terminal: String,
    },
    WorktreeCreated {
        from: String,
    },
    WorktreeRemoved,
    SessionOpened {
        session: String,
    },
    SessionClosed {
        session: String,
    },
    GitChanged,
    ReviewSent {
        merge_request: u64,
        verdict: ReviewVerdict,
    },
    CiFailed {
        merge_request: u64,
        pipeline: u64,
    },
    CiPassed {
        merge_request: u64,
        pipeline: u64,
    },
    MergeRequestOpened {
        merge_request: u64,
    },
}

impl AppEvent {
    #[must_use]
    pub const fn kind(&self) -> AppEventKind {
        match self {
            Self::AgentStarted { .. } => AppEventKind::AgentStarted,
            Self::AgentWaiting { .. } => AppEventKind::AgentWaiting,
            Self::AgentFinished { .. } => AppEventKind::AgentFinished,
            Self::WorktreeCreated { .. } => AppEventKind::WorktreeCreated,
            Self::WorktreeRemoved => AppEventKind::WorktreeRemoved,
            Self::SessionOpened { .. } => AppEventKind::SessionOpened,
            Self::SessionClosed { .. } => AppEventKind::SessionClosed,
            Self::GitChanged => AppEventKind::GitChanged,
            Self::ReviewSent { .. } => AppEventKind::ReviewSent,
            Self::CiFailed { .. } => AppEventKind::CiFailed,
            Self::CiPassed { .. } => AppEventKind::CiPassed,
            Self::MergeRequestOpened { .. } => AppEventKind::MergeRequestOpened,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SubscribeEventsParams {
    pub kinds: BTreeSet<AppEventKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppEventParams {
    pub version: u32,
    pub environment: String,
    #[serde(default)]
    pub group: Option<String>,
    pub event: AppEvent,
}

request!(
    SubscribeEvents,
    "events/subscribe",
    App,
    SubscribeEventsParams,
    Empty
);
notification!(
    AppEventNotification,
    "events/event",
    Program,
    AppEventParams
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Notification, Party, Request};
    use serde_json::json;

    #[test]
    fn a_subscription_and_an_event_survive_the_wire() -> serde_json::Result<()> {
        assert_eq!(SubscribeEvents::METHOD, "events/subscribe");
        assert_eq!(SubscribeEvents::ANSWERED_BY, Party::App);
        assert_eq!(AppEventNotification::RECEIVED_BY, Party::Program);
        let subscription: SubscribeEventsParams =
            serde_json::from_value(json!({ "kinds": ["ci_failed", "agent_waiting"] }))?;
        assert_eq!(
            subscription.kinds.into_iter().collect::<Vec<_>>(),
            vec![AppEventKind::AgentWaiting, AppEventKind::CiFailed]
        );
        let params = AppEventParams {
            version: 1,
            environment: "main".to_owned(),
            group: Some("3".to_owned()),
            event: AppEvent::CiFailed {
                merge_request: 12,
                pipeline: 40,
            },
        };
        let wire = serde_json::to_value(&params)?;
        assert_eq!(wire["event"]["kind"], "ci_failed");
        assert_eq!(serde_json::from_value::<AppEventParams>(wire)?, params);
        assert_eq!(params.event.kind(), AppEventKind::CiFailed);
        Ok(())
    }

    #[test]
    fn an_unknown_kind_is_refused_rather_than_guessed() {
        let unknown = serde_json::from_value::<SubscribeEventsParams>(json!({ "kinds": ["x"] }));
        assert!(unknown.is_err());
    }
}
