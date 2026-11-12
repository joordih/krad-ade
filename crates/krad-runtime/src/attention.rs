use krad_domain::{
    AttentionKind, AttentionLog, AttentionQueue, AttentionRecord, AttentionTarget, InboxFilter,
    StateChange, Timestamp,
};
use krad_protocol::{
    AttentionCommand, AttentionNotice, AttentionSnapshot, AttentionWaiting, EventKind, Reply,
};
use std::time::{Instant, SystemTime};

#[derive(Debug, Default)]
pub(crate) struct Attention {
    queue: AttentionQueue,
    log: AttentionLog,
}

type Raised<'a> = Option<(AttentionKind, &'a str)>;

fn wall_time(at: Instant) -> Timestamp {
    let now = SystemTime::now();
    Timestamp::from(now.checked_sub(at.elapsed()).unwrap_or(now))
}

impl Attention {
    pub(crate) fn execute(
        &mut self,
        command: &AttentionCommand,
        outbox: &mut Vec<EventKind>,
    ) -> Option<Reply> {
        let target = match command {
            AttentionCommand::Observe { target, .. }
            | AttentionCommand::Raise { target, .. }
            | AttentionCommand::Seen { target }
            | AttentionCommand::Dismiss { target } => target,
            AttentionCommand::ClearHistory => {
                self.log.clear();
                outbox.push(EventKind::AttentionHistoryCleared);
                return Some(Reply::Accepted);
            }
            AttentionCommand::Unknown => return None,
        };
        let queued = self.queue.contains(target);
        let raised = self.apply(command);
        self.report(target, queued, raised, outbox);
        Some(Reply::Accepted)
    }

    fn apply<'a>(&mut self, command: &'a AttentionCommand) -> Raised<'a> {
        match command {
            AttentionCommand::Observe {
                target,
                previous,
                next,
                waiting_for,
                sight,
                title,
            } => self
                .queue
                .observe(StateChange {
                    target: target.clone(),
                    previous: *previous,
                    next: *next,
                    waiting_for: *waiting_for,
                    sight: *sight,
                    at: Instant::now(),
                })
                .map(|kind| (kind, title.as_str())),
            AttentionCommand::Raise {
                target,
                kind,
                sight,
                title,
            } => self
                .queue
                .raise(target.clone(), *kind, *sight, Instant::now())
                .map(|kind| (kind, title.as_str())),
            AttentionCommand::Seen { target } => {
                self.queue.seen(target);
                None
            }
            AttentionCommand::Dismiss { target } => {
                self.queue.forget(target);
                None
            }
            AttentionCommand::ClearHistory | AttentionCommand::Unknown => None,
        }
    }

    fn report(
        &mut self,
        target: &AttentionTarget,
        queued: bool,
        raised: Raised<'_>,
        outbox: &mut Vec<EventKind>,
    ) {
        let Some((kind, title)) = raised else {
            if queued && !self.queue.contains(target) {
                outbox.push(EventKind::AttentionLeft {
                    target: target.clone(),
                });
            }
            return;
        };
        self.log.record(AttentionRecord {
            target: target.clone(),
            kind,
            title: title.to_owned(),
            at: Instant::now(),
        });
        outbox.push(EventKind::AttentionRaised(AttentionNotice {
            target: target.clone(),
            kind,
            title: title.to_owned(),
            at: Timestamp::now(),
        }));
    }

    pub(crate) fn snapshot(&self) -> AttentionSnapshot {
        AttentionSnapshot {
            waiting: self
                .queue
                .ordered()
                .into_iter()
                .map(|entry| AttentionWaiting {
                    target: entry.target.clone(),
                    kind: entry.kind,
                    since: wall_time(entry.since),
                })
                .collect(),
            history: self
                .log
                .filtered(InboxFilter::All)
                .map(|record| AttentionNotice {
                    target: record.target.clone(),
                    kind: record.kind,
                    title: record.title.clone(),
                    at: wall_time(record.at),
                })
                .collect(),
        }
    }
}
