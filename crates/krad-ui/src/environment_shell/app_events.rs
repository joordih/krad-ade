use gpui_kit::Task;
use krad_domain::{AppEvent, EnvironmentName, EventRecord, GroupId, WebhookId};
use krad_protocol::{Command, EventsCommand};
use std::collections::BTreeSet;

use super::EnvironmentShell;

#[derive(Default)]
pub(super) struct AppEventsUi {
    delivered: BTreeSet<WebhookId>,
    pub(super) webhook_secrets: Option<Task<()>>,
}

impl EnvironmentShell {
    pub(super) fn publish_event(
        &mut self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        event: AppEvent,
    ) {
        self.runtime
            .client
            .post(Command::Events(EventsCommand::Publish {
                environment: environment.clone(),
                group,
                event,
                delivered: self.app_events.delivered.clone(),
            }));
    }

    pub(super) fn use_webhooks(&self) {
        self.runtime
            .client
            .post(Command::Events(EventsCommand::UseWebhooks {
                webhooks: self.settings.webhooks.clone(),
            }));
    }

    pub(super) fn app_event_heard(&mut self, record: &EventRecord) {
        self.announce_app_event(record);
        self.variant_event_heard(record);
    }

    pub(super) fn with_webhooks_delivered(
        &mut self,
        delivered: BTreeSet<WebhookId>,
        publish: impl FnOnce(&mut Self),
    ) {
        self.app_events.delivered = delivered;
        publish(self);
        self.app_events.delivered.clear();
    }
}
