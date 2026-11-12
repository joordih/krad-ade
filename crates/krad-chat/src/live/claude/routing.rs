use krad_domain::ChatEvent;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::SystemTime;

use super::{live_event, Awaited, AwaitedReply, KnownSession, UsageAsk};
use crate::cloud::LineOrigin;
use crate::control::{route, LiveEvent, Routed};
use crate::parse::{claude, claude_limit_alert, claude_plan_usage, claude_spend, usage_failure_of};
use crate::rewind::{files_reply, RewindError};

pub(in crate::live) struct Routing {
    pub(in crate::live) init_id: String,
    pub(in crate::live) pending: Arc<AtomicUsize>,
    pub(super) awaited: AwaitedReply,
    pub(super) session: KnownSession,
    pub(super) usage: UsageAsk,
}

impl Routing {
    #[cfg(test)]
    pub(in crate::live) fn new(init_id: &str, pending: usize) -> Self {
        Self {
            init_id: init_id.to_owned(),
            pending: Arc::new(AtomicUsize::new(pending)),
            awaited: Arc::default(),
            session: Arc::default(),
            usage: UsageAsk::default(),
        }
    }

    fn take_awaited(&self, request_id: &str) -> Option<Awaited> {
        let mut awaited = self.awaited.lock().unwrap_or_else(PoisonError::into_inner);
        awaited.take_if(|awaited| awaited.request_id == request_id)
    }

    pub(super) fn abandon_reply(&self) {
        let awaited = self
            .awaited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(awaited) = awaited {
            awaited
                .reply
                .send(Err(RewindError::Stopped))
                .unwrap_or_default();
        }
    }

    pub(in crate::live) fn route_line(
        &self,
        line: &str,
        origin: LineOrigin,
        sink: &mut impl FnMut(LiveEvent),
    ) {
        let routed = route(line, &self.init_id);
        if origin.is_replayed() && !matches!(routed, Some(Routed::Ask(_))) {
            return;
        }
        let value = match routed {
            None | Some(Routed::Control) => return,
            Some(Routed::Ask(ask)) => return sink(LiveEvent::Ask(ask)),
            Some(Routed::Ready(catalog)) => return sink(LiveEvent::Ready(catalog)),
            Some(Routed::Replied { request_id, reply }) if UsageAsk::answers(&request_id) => {
                let read = reply
                    .map_err(|error| usage_failure_of(&error))
                    .and_then(|reply| claude_plan_usage(&reply));
                match read {
                    Ok(usage) => sink(LiveEvent::Usage(usage)),
                    Err(failure) => tracing::debug!(?failure, "the chat could not read plan usage"),
                }
                return;
            }
            Some(Routed::Replied { request_id, reply }) => {
                if let Some(awaited) = self.take_awaited(&request_id) {
                    awaited.reply.send(files_reply(reply)).unwrap_or_default();
                }
                return;
            }
            Some(Routed::Conversation(value)) => value,
        };
        if let Some(spent) = claude_spend(&value) {
            sink(LiveEvent::Spent(spent));
        }
        if let Some(alert) = claude_limit_alert(&value, SystemTime::now()) {
            return sink(LiveEvent::Limit(alert.into()));
        }
        for event in claude(&value) {
            match &event {
                ChatEvent::Done | ChatEvent::Error(_) => {
                    self.usage.ask();
                    self.pending
                        .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                            count.checked_sub(1)
                        })
                        .unwrap_or_default();
                }
                ChatEvent::TurnStarted => {
                    self.pending
                        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                        .unwrap_or_default();
                }
                ChatEvent::Session(id) => {
                    *self.session.lock().unwrap_or_else(PoisonError::into_inner) = Some(id.clone());
                }
                _ => {}
            }
            sink(live_event(event));
        }
    }
}
