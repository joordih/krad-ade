use krad_domain::{
    DeliveryOutcome, EventRecord, InstallId, RunnerWebhooks, SyncedWebhook, WebhookId,
    WebhookSigning,
};
use krad_host::runner::{events_file, webhooks_file};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::time::{Duration, Instant};

use super::{queued_events, runner_record, QueuedEvent};
use crate::webhook::{post_webhook, webhook_agent};

const CLI_WAIT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_PAUSE: Duration = Duration::from_millis(250);
const ATTEMPTS: u32 = 2;
const SENDER_THREAD_NAME: &str = "krad-ade-webhook";

struct Delivery {
    install: InstallId,
    hook: SyncedWebhook,
    record: EventRecord,
}

type Sent = HashMap<InstallId, BTreeSet<WebhookId>>;

fn read_webhooks(file: &Path) -> Option<RunnerWebhooks> {
    let text = std::fs::read_to_string(file).ok()?;
    serde_json::from_str(&text).ok()
}

fn deliveries_for(runner_dir: &Path, install: &InstallId, queued: &QueuedEvent) -> Vec<Delivery> {
    let Some(webhooks) = read_webhooks(&webhooks_file(runner_dir, install)) else {
        return Vec::new();
    };
    let earlier = std::fs::read_to_string(events_file(runner_dir, install))
        .map_or_default(|text| queued_events(&text));
    let Some(record) = runner_record(&webhooks, &earlier, queued) else {
        return Vec::new();
    };
    webhooks
        .wanting(record.event.kind())
        .filter(|synced| synced.hook.signing == WebhookSigning::Unsigned || synced.secret.is_some())
        .map(|synced| Delivery {
            install: install.clone(),
            hook: synced.clone(),
            record: record.clone(),
        })
        .collect()
}

fn deliver(delivery: &Delivery, deadline: Instant) -> DeliveryOutcome {
    let agent = webhook_agent(REQUEST_TIMEOUT);
    let mut attempt = 1;
    loop {
        let outcome = post_webhook(
            &agent,
            &delivery.hook.hook,
            &delivery.record,
            delivery.hook.secret.as_ref(),
        )
        .unwrap_or_else(|error| DeliveryOutcome::from(&error));
        let retry_fits = Instant::now() + RETRY_PAUSE + REQUEST_TIMEOUT <= deadline;
        if outcome != DeliveryOutcome::Failed || attempt >= ATTEMPTS || !retry_fits {
            return outcome;
        }
        attempt += 1;
        std::thread::sleep(RETRY_PAUSE);
    }
}

fn spawn_delivery(
    delivery: Delivery,
    deadline: Instant,
    done: Sender<(InstallId, WebhookId, DeliveryOutcome)>,
) -> bool {
    std::thread::Builder::new()
        .name(SENDER_THREAD_NAME.to_owned())
        .spawn(move || {
            let outcome = deliver(&delivery, deadline);
            done.send((delivery.install, delivery.hook.hook.id, outcome))
                .unwrap_or_default();
        })
        .is_ok()
}

fn send_within(deliveries: Vec<Delivery>, wait: Duration) -> Sent {
    let deadline = Instant::now() + wait;
    let (done, results) = channel();
    let started = deliveries
        .into_iter()
        .map(|delivery| spawn_delivery(delivery, deadline, done.clone()))
        .filter(|started| *started)
        .count();
    drop(done);
    let mut sent = Sent::new();
    for _ in 0..started {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok((install, hook, outcome)) = results.recv_timeout(left) else {
            break;
        };
        if outcome != DeliveryOutcome::Failed {
            sent.entry(install).or_default().insert(hook);
        }
    }
    sent
}

pub(super) fn queue_lines(
    runner_dir: &Path,
    installs: &[InstallId],
    queued: &QueuedEvent,
) -> Vec<(InstallId, String)> {
    let deliveries: Vec<Delivery> = installs
        .iter()
        .flat_map(|install| deliveries_for(runner_dir, install, queued))
        .collect();
    let mut sent = if deliveries.is_empty() {
        Sent::new()
    } else {
        send_within(deliveries, CLI_WAIT)
    };
    installs
        .iter()
        .filter_map(|install| {
            let event = QueuedEvent {
                webhooked: sent.remove(install).unwrap_or_default(),
                ..queued.clone()
            };
            let line = serde_json::to_string(&event).ok()?;
            Some((install.clone(), line))
        })
        .collect()
}

#[cfg(test)]
mod tests;
