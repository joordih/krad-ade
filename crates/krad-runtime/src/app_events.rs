mod webhooks;

use krad_domain::{
    AppEvent, EnvironmentName, EventRecord, GroupId, Webhook, WebhookId, WebhookSettings,
};
use krad_protocol::{ErrorCode, EventKind, EventsCommand, Reply};
use krad_store::{events_dir, EventJournal, JournalSend, Secrets};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::time::SystemTime;

use crate::input::Input;
use crate::runtime::refused;
use webhooks::{Delivery, WebhookSender};

const NO_SENDER: &str = "this runtime sends no webhooks";
const NO_HOOK: &str = "no webhook has that id";

#[derive(Clone, Debug)]
pub struct EventsSetup {
    pub journal: Option<PathBuf>,
}

impl EventsSetup {
    #[must_use]
    pub fn in_config_folder() -> Self {
        Self {
            journal: events_dir(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AppEvents {
    journal: Option<EventJournal>,
    sender: Option<WebhookSender>,
    webhooks: WebhookSettings,
}

fn undelivered<'a>(
    wanting: impl Iterator<Item = &'a Webhook>,
    delivered: &'a BTreeSet<WebhookId>,
) -> impl Iterator<Item = &'a Webhook> {
    wanting.filter(move |hook| !delivered.contains(&hook.id))
}

impl AppEvents {
    pub(crate) fn start(
        setup: Option<EventsSetup>,
        secrets: &Secrets,
        report: &SyncSender<Input>,
    ) -> Self {
        let Some(setup) = setup else {
            return Self::default();
        };
        Self {
            journal: setup.journal.and_then(EventJournal::spawn),
            sender: WebhookSender::spawn(secrets.clone(), report.clone()),
            webhooks: WebhookSettings::default(),
        }
    }

    pub(crate) fn execute(
        &mut self,
        command: &EventsCommand,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        match command {
            EventsCommand::Publish {
                environment,
                group,
                event,
                delivered,
            } => self.publish(environment, *group, event, delivered, outbox),
            EventsCommand::UseWebhooks { webhooks } => self.use_webhooks(webhooks),
            EventsCommand::SaveWebhook { hook } => {
                self.webhooks.save(hook.clone());
                self.changed(hook.id, Some(hook));
            }
            EventsCommand::RemoveWebhook { hook } => {
                self.webhooks.remove(*hook);
                self.changed(*hook, None);
            }
            EventsCommand::TestWebhook { hook, environment } => {
                return Some(self.test(*hook, environment));
            }
            EventsCommand::WatchCodeHosts { .. } | EventsCommand::Unknown => return None,
        }
        Some(Reply::Accepted)
    }

    pub(crate) fn wants_code_host(&self) -> bool {
        self.webhooks.wants_code_host()
    }

    pub(crate) fn publish_own(
        &self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        event: &AppEvent,
        outbox: &mut Vec<EventKind>,
    ) {
        self.publish(environment, group, event, &BTreeSet::new(), outbox);
    }

    fn publish(
        &self,
        environment: &EnvironmentName,
        group: Option<GroupId>,
        event: &AppEvent,
        delivered: &BTreeSet<WebhookId>,
        outbox: &mut Vec<EventKind>,
    ) {
        let record = EventRecord::new(SystemTime::now(), environment.clone(), group, event.clone());
        let kind = record.event.kind();
        tracing::trace!(?kind, "app event");
        if let Some(sender) = &self.sender {
            for hook in undelivered(self.webhooks.wanting(kind), delivered) {
                sender.send(Delivery {
                    hook: hook.clone(),
                    record: record.clone(),
                });
            }
        }
        if let Some(journal) = &self.journal {
            match journal.record(record.clone()) {
                JournalSend::Queued => {}
                JournalSend::Full => tracing::warn!(?kind, "event journal full; event not kept"),
                JournalSend::Stopped => tracing::debug!("event journal stopped"),
            }
        }
        outbox.push(EventKind::AppEvent(record));
    }

    fn use_webhooks(&mut self, webhooks: &WebhookSettings) {
        let gone: Vec<WebhookId> = self
            .webhooks
            .hooks
            .iter()
            .map(|hook| hook.id)
            .filter(|id| webhooks.find(*id).is_none())
            .collect();
        for id in gone {
            self.changed(id, None);
        }
        for hook in &webhooks.hooks {
            self.changed(hook.id, Some(hook));
        }
        self.webhooks = webhooks.clone();
    }

    fn changed(&self, id: WebhookId, now: Option<&Webhook>) {
        if let Some(sender) = &self.sender {
            sender.changed(id, now.cloned());
        }
    }

    fn test(&self, hook: WebhookId, environment: &EnvironmentName) -> Reply {
        let Some(sender) = &self.sender else {
            return refused(ErrorCode::Unsupported, NO_SENDER);
        };
        let Some(hook) = self.webhooks.find(hook) else {
            return refused(ErrorCode::NotFound, NO_HOOK);
        };
        let record = EventRecord::new(SystemTime::now(), environment.clone(), None, AppEvent::Ping);
        sender.test(Delivery {
            hook: hook.clone(),
            record,
        });
        Reply::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhooks_the_runner_delivered_are_left_out() -> Result<(), Box<dyn std::error::Error>> {
        let hook = |id: u64| -> Result<Webhook, Box<dyn std::error::Error>> {
            Ok(Webhook {
                id: WebhookId(id),
                name: "ci".parse()?,
                url: "https://hooks.example.test/krad".parse()?,
                kinds: BTreeSet::new(),
                signing: krad_domain::WebhookSigning::Unsigned,
            })
        };
        let hooks = [hook(1)?, hook(2)?];
        let left = |delivered: &BTreeSet<WebhookId>| -> Vec<WebhookId> {
            undelivered(hooks.iter(), delivered)
                .map(|hook| hook.id)
                .collect()
        };
        assert_eq!(left(&BTreeSet::new()), vec![WebhookId(1), WebhookId(2)]);
        assert_eq!(
            left(&[WebhookId(2)].into_iter().collect()),
            vec![WebhookId(1)]
        );
        Ok(())
    }
}
