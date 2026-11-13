use krad_agents::{webhook_request, WebhookPostError, WebhookRequest};
use krad_domain::{
    next_delivery_step, DeliveryOutcome, DeliveryStep, EventRecord, Webhook, WebhookId,
    WebhookSecret, WebhookSigning,
};
use krad_protocol::WebhookTest;
use krad_store::Secrets;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};
use ureq::Agent;

use crate::input::Input;

const SENDER_THREAD_NAME: &str = "krad-ade-webhooks";
const SENDER_QUEUE: usize = 128;
const PENDING_LIMIT: usize = 256;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub(crate) struct Delivery {
    pub(crate) hook: Webhook,
    pub(crate) record: EventRecord,
}

enum Order {
    Deliver(Delivery),
    Test(Delivery),
    Changed(WebhookId, Option<Webhook>),
}

#[derive(Debug)]
pub(crate) struct WebhookSender {
    queue: SyncSender<Order>,
}

impl WebhookSender {
    pub(crate) fn spawn(secrets: Secrets, report: SyncSender<Input>) -> Option<Self> {
        let (queue, inbox) = sync_channel::<Order>(SENDER_QUEUE);
        std::thread::Builder::new()
            .name(SENDER_THREAD_NAME.to_owned())
            .spawn(move || serve(&inbox, secrets, &report))
            .map_err(|error| tracing::warn!(%error, "the webhook sender did not start"))
            .ok()?;
        Some(Self { queue })
    }

    pub(crate) fn send(&self, delivery: Delivery) {
        let hook = delivery.hook.id;
        if self.queue.try_send(Order::Deliver(delivery)).is_err() {
            tracing::warn!(webhook = %hook, "webhook queue full; event dropped");
        }
    }

    pub(crate) fn test(&self, delivery: Delivery) {
        let hook = delivery.hook.id;
        if self.queue.try_send(Order::Test(delivery)).is_err() {
            tracing::warn!(webhook = %hook, "webhook queue full; test dropped");
        }
    }

    pub(crate) fn changed(&self, hook: WebhookId, now: Option<Webhook>) {
        if self.queue.try_send(Order::Changed(hook, now)).is_err() {
            tracing::debug!(webhook = %hook, "webhook change not queued");
        }
    }
}

struct Pending {
    hook: WebhookId,
    record: EventRecord,
    attempt: u32,
    due: Instant,
}

#[derive(Default)]
struct Retries {
    pending: VecDeque<Pending>,
}

impl Retries {
    fn push(&mut self, pending: Pending) {
        if self.pending.len() >= PENDING_LIMIT {
            if let Some(dropped) = self.pending.pop_front() {
                tracing::warn!(webhook = %dropped.hook, "webhook retry dropped");
            }
        }
        self.pending.push_back(pending);
    }

    fn wait(&self, now: Instant) -> Option<Duration> {
        self.pending
            .iter()
            .map(|pending| pending.due.saturating_duration_since(now))
            .min()
    }

    fn take_due(&mut self, now: Instant) -> Vec<Pending> {
        let (due, later): (VecDeque<Pending>, VecDeque<Pending>) = self
            .pending
            .drain(..)
            .partition(|pending| pending.due <= now);
        self.pending = later;
        due.into()
    }

    fn forget(&mut self, hook: WebhookId) {
        self.pending.retain(|pending| pending.hook != hook);
    }

    fn settle(&mut self, pending: Pending, outcome: DeliveryOutcome, now: Instant) -> DeliveryStep {
        let step = next_delivery_step(pending.attempt, outcome);
        if let DeliveryStep::RetryAfter(wait) = step {
            self.push(Pending {
                attempt: pending.attempt.saturating_add(1),
                due: now + wait,
                ..pending
            });
        }
        step
    }
}

struct HookSecrets {
    store: Secrets,
    known: HashMap<WebhookId, Option<WebhookSecret>>,
}

fn find_secret(store: &Secrets, hook: WebhookId) -> Option<WebhookSecret> {
    match store.find_webhook_secret(hook) {
        Ok(found) => found.map(|(secret, _)| secret),
        Err(error) => {
            tracing::warn!(webhook = %hook, %error, "webhook secret unreadable");
            None
        }
    }
}

impl HookSecrets {
    fn new(store: Secrets) -> Self {
        Self {
            store,
            known: HashMap::new(),
        }
    }

    fn of(&mut self, hook: WebhookId) -> Option<WebhookSecret> {
        let store = &self.store;
        self.known
            .entry(hook)
            .or_insert_with(|| find_secret(store, hook))
            .clone()
    }

    fn fresh(&self, hook: &Webhook) -> Option<WebhookSecret> {
        (hook.signing == WebhookSigning::Signed)
            .then(|| find_secret(&self.store, hook.id))
            .flatten()
    }
}

fn next_order(inbox: &Receiver<Order>, wait: Option<Duration>) -> Result<Option<Order>, ()> {
    match wait {
        None => inbox.recv().map(Some).map_err(drop),
        Some(wait) => match inbox.recv_timeout(wait) {
            Ok(order) => Ok(Some(order)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(()),
        },
    }
}

#[derive(Default)]
struct Outbox {
    retries: Retries,
    hooks: HashMap<WebhookId, Webhook>,
}

impl Outbox {
    fn deliver(&mut self, Delivery { hook, record }: Delivery, now: Instant) {
        let id = hook.id;
        self.hooks.insert(id, hook);
        self.retries.push(Pending {
            hook: id,
            record,
            attempt: 1,
            due: now,
        });
    }

    fn changed(&mut self, id: WebhookId, now: Option<Webhook>, secrets: &mut HookSecrets) {
        secrets.known.remove(&id);
        if let Some(hook) = now {
            self.hooks.insert(id, hook);
        } else {
            self.hooks.remove(&id);
            self.retries.forget(id);
        }
    }

    fn due(&mut self, now: Instant) -> Vec<(Webhook, Pending)> {
        self.retries
            .take_due(now)
            .into_iter()
            .filter_map(|pending| Some((self.hooks.get(&pending.hook)?.clone(), pending)))
            .collect()
    }
}

fn test(agent: &Agent, delivery: &Delivery, secrets: &HookSecrets, report: &SyncSender<Input>) {
    let secret = secrets.fresh(&delivery.hook);
    let outcome = post(agent, &delivery.hook, &delivery.record, secret.as_ref());
    let tested = WebhookTest {
        hook: delivery.hook.id,
        outcome,
    };
    if report.send(Input::WebhookTested(tested)).is_err() {
        tracing::debug!("a webhook test ended after the runtime stopped");
    }
}

fn serve(inbox: &Receiver<Order>, store: Secrets, report: &SyncSender<Input>) {
    let agent = webhook_agent(REQUEST_TIMEOUT);
    let mut outbox = Outbox::default();
    let mut secrets = HookSecrets::new(store);
    while let Ok(order) = next_order(inbox, outbox.retries.wait(Instant::now())) {
        match order {
            Some(Order::Test(delivery)) => test(&agent, &delivery, &secrets, report),
            Some(Order::Deliver(delivery)) => outbox.deliver(delivery, Instant::now()),
            Some(Order::Changed(id, now)) => outbox.changed(id, now, &mut secrets),
            None => {}
        }
        for (hook, pending) in outbox.due(Instant::now()) {
            let secret = secrets.of(hook.id);
            let outcome = post(&agent, &hook, &pending.record, secret.as_ref());
            match outbox.retries.settle(pending, outcome, Instant::now()) {
                DeliveryStep::Done => tracing::trace!(webhook = %hook.id, "webhook delivered"),
                DeliveryStep::RetryAfter(wait) => {
                    tracing::debug!(webhook = %hook.id, ?wait, "webhook delivery retried");
                }
                DeliveryStep::GiveUp => {
                    tracing::warn!(webhook = %hook.id, "webhook delivery failed");
                }
            }
        }
    }
}

fn webhook_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

fn send_request(agent: &Agent, request: &WebhookRequest) -> DeliveryOutcome {
    let sending = request.headers.iter().fold(
        agent.post(request.url.as_str()),
        |sending, (name, value)| sending.header(*name, value),
    );
    match sending.send(request.body.as_bytes()) {
        Ok(response) => DeliveryOutcome::of_status(response.status().as_u16()),
        Err(error) => {
            tracing::debug!(%error, "webhook not reached");
            DeliveryOutcome::Failed
        }
    }
}

#[tracing::instrument(skip_all, fields(webhook = %hook.id))]
fn post(
    agent: &Agent,
    hook: &Webhook,
    record: &EventRecord,
    secret: Option<&WebhookSecret>,
) -> DeliveryOutcome {
    match webhook_request(hook, record, secret) {
        Ok(request) => send_request(agent, &request),
        Err(error @ WebhookPostError::NoSecret) => {
            tracing::warn!(%error, "webhook not sent");
            DeliveryOutcome::from(&error)
        }
        Err(error) => {
            tracing::debug!(%error, "webhook not sent");
            DeliveryOutcome::from(&error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AppEvent, EnvironmentName, WEBHOOK_FIRST_BACKOFF, WEBHOOK_MAX_ATTEMPTS};
    use std::time::SystemTime;

    fn delivery(id: u64) -> Delivery {
        Delivery {
            hook: Webhook {
                id: WebhookId(id),
                name: "ci".parse().unwrap_or_else(|_| unreachable!("a name")),
                url: "https://hooks.example.test/x"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("a url")),
                kinds: std::collections::BTreeSet::new(),
                signing: WebhookSigning::Unsigned,
            },
            record: EventRecord::new(
                SystemTime::UNIX_EPOCH,
                EnvironmentName::new("main".to_owned()),
                None,
                AppEvent::Ping,
            ),
        }
    }

    fn pending(id: u64, due: Instant) -> Pending {
        Pending {
            hook: WebhookId(id),
            record: delivery(id).record,
            attempt: 1,
            due,
        }
    }

    #[test]
    fn a_failed_delivery_waits_its_backoff_then_gives_up() {
        let now = Instant::now();
        let mut retries = Retries::default();
        retries.push(pending(1, now));
        let mut steps = Vec::new();
        let mut clock = now;
        while let Some(due) = retries.take_due(clock).pop() {
            steps.push(retries.settle(due, DeliveryOutcome::Failed, clock));
            clock += retries.wait(clock).unwrap_or_default();
        }
        assert_eq!(
            steps.first(),
            Some(&DeliveryStep::RetryAfter(WEBHOOK_FIRST_BACKOFF))
        );
        assert_eq!(steps.last(), Some(&DeliveryStep::GiveUp));
        assert_eq!(
            steps.len(),
            usize::try_from(WEBHOOK_MAX_ATTEMPTS).unwrap_or(0)
        );
        assert!(retries.wait(clock).is_none());
    }

    #[test]
    fn only_due_deliveries_are_taken_and_the_queue_is_bounded() {
        let now = Instant::now();
        let mut retries = Retries::default();
        retries.push(pending(1, now));
        retries.push(pending(2, now + WEBHOOK_FIRST_BACKOFF));
        let taken: Vec<WebhookId> = retries
            .take_due(now)
            .into_iter()
            .map(|pending| pending.hook)
            .collect();
        assert_eq!(taken, vec![WebhookId(1)]);
        assert_eq!(retries.wait(now), Some(WEBHOOK_FIRST_BACKOFF));
        for id in 0..PENDING_LIMIT {
            retries.push(pending(u64::try_from(id).unwrap_or(0) + 10, now));
        }
        assert_eq!(retries.pending.len(), PENDING_LIMIT);
        assert!(retries
            .pending
            .iter()
            .all(|pending| pending.hook != WebhookId(2)));
    }

    #[test]
    fn a_retry_goes_to_the_hook_as_it_is_now_and_a_removed_hook_gets_none() {
        let now = Instant::now();
        let mut outbox = Outbox::default();
        let mut secrets = HookSecrets::new(Secrets::in_memory());
        outbox.deliver(delivery(1), now);
        outbox.deliver(delivery(2), now);
        let mut moved = delivery(1).hook;
        moved.url = "https://hooks.example.test/moved"
            .parse()
            .unwrap_or_else(|_| unreachable!("a url"));
        outbox.changed(WebhookId(1), Some(moved), &mut secrets);
        outbox.changed(WebhookId(2), None, &mut secrets);
        let due = outbox.due(now);
        let sent: Vec<(WebhookId, &str)> = due
            .iter()
            .map(|(hook, _)| (hook.id, hook.url.as_str()))
            .collect();
        assert_eq!(
            sent,
            vec![(WebhookId(1), "https://hooks.example.test/moved")]
        );
        assert!(outbox.retries.wait(now).is_none());
    }
}
