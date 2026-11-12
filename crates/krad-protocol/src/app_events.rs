use krad_domain::{
    AppEvent, DeliveryOutcome, EnvironmentName, GroupId, Webhook, WebhookId, WebhookSettings,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum EventsCommand {
    Publish {
        environment: EnvironmentName,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<GroupId>,
        event: AppEvent,
        #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
        delivered: BTreeSet<WebhookId>,
    },
    UseWebhooks {
        webhooks: WebhookSettings,
    },
    SaveWebhook {
        hook: Webhook,
    },
    RemoveWebhook {
        hook: WebhookId,
    },
    TestWebhook {
        hook: WebhookId,
        environment: EnvironmentName,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WebhookTest {
    pub hook: WebhookId,
    pub outcome: DeliveryOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Event, EventKind, Seq};
    use krad_domain::{EventRecord, TerminalId};
    use std::time::UNIX_EPOCH;

    #[test]
    fn a_publish_command_has_a_stable_wire_shape() -> serde_json::Result<()> {
        let command = Command::Events(EventsCommand::Publish {
            environment: EnvironmentName::new("main".to_owned()),
            group: None,
            event: AppEvent::AgentFinished {
                terminal: TerminalId(2),
            },
            delivered: BTreeSet::new(),
        });
        let json = serde_json::to_string(&command)?;
        assert_eq!(
            json,
            r#"{"area":"events","verb":"publish","environment":"main","event":{"kind":"agent_finished","terminal":2}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&json)?, command);
        let newer: Command = serde_json::from_str(r#"{"area":"events","verb":"mute"}"#)?;
        assert_eq!(newer, Command::Events(EventsCommand::Unknown));
        Ok(())
    }

    #[test]
    fn published_records_and_webhook_tests_round_trip() -> serde_json::Result<()> {
        let events = [
            EventKind::AppEvent(EventRecord::new(
                UNIX_EPOCH,
                EnvironmentName::new("main".to_owned()),
                Some(GroupId(3)),
                AppEvent::GitChanged,
            )),
            EventKind::WebhookTested(WebhookTest {
                hook: WebhookId(4),
                outcome: DeliveryOutcome::Refused,
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
