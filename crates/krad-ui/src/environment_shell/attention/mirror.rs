use krad_domain::{
    AttentionEntry, AttentionLog, AttentionQueue, AttentionRecord, AttentionTarget, Timestamp,
};
use krad_protocol::{AttentionNotice, AttentionSnapshot};
use std::time::{Instant, SystemTime};

#[derive(Default)]
pub(in crate::environment_shell) struct AttentionMirror {
    pub(in crate::environment_shell) queue: AttentionQueue,
    pub(super) log: AttentionLog,
}

fn instant_of(at: Timestamp) -> Instant {
    let now = Instant::now();
    let age = SystemTime::now()
        .duration_since(SystemTime::from(at))
        .unwrap_or_default();
    now.checked_sub(age).unwrap_or(now)
}

impl AttentionMirror {
    pub(in crate::environment_shell) fn raised(&mut self, notice: &AttentionNotice) {
        let at = instant_of(notice.at);
        self.queue.insert(AttentionEntry {
            target: notice.target.clone(),
            kind: notice.kind,
            since: at,
        });
        self.log.record(AttentionRecord {
            target: notice.target.clone(),
            kind: notice.kind,
            title: notice.title.clone(),
            at,
        });
    }

    pub(in crate::environment_shell) fn left(&mut self, target: &AttentionTarget) {
        self.queue.forget(target);
    }

    pub(in crate::environment_shell) fn history_cleared(&mut self) {
        self.log.clear();
    }

    pub(in crate::environment_shell) fn replace(&mut self, snapshot: AttentionSnapshot) {
        self.queue = AttentionQueue::default();
        for waiting in snapshot.waiting {
            self.queue.insert(AttentionEntry {
                target: waiting.target,
                kind: waiting.kind,
                since: instant_of(waiting.since),
            });
        }
        self.log = AttentionLog::default();
        for notice in snapshot.history.into_iter().rev() {
            self.log.record(AttentionRecord {
                at: instant_of(notice.at),
                target: notice.target,
                kind: notice.kind,
                title: notice.title,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AttentionKind, EnvironmentName, InboxFilter, TerminalId};
    use krad_protocol::AttentionWaiting;

    fn target(id: u64) -> AttentionTarget {
        AttentionTarget::new(EnvironmentName::new("main".to_owned()), TerminalId(id))
    }

    fn notice(id: u64, kind: AttentionKind) -> AttentionNotice {
        AttentionNotice {
            target: target(id),
            kind,
            title: format!("agent {id}"),
            at: Timestamp::now(),
        }
    }

    #[test]
    fn raised_and_left_notices_follow_the_runtime_queue_and_history() {
        let mut mirror = AttentionMirror::default();
        mirror.raised(&notice(1, AttentionKind::Finished));
        mirror.raised(&notice(1, AttentionKind::Permission));
        assert_eq!(
            mirror.queue.kind_of(&target(1)),
            Some(AttentionKind::Permission)
        );
        assert_eq!(mirror.log.count(InboxFilter::All), 2);
        mirror.left(&target(1));
        assert!(mirror.queue.is_empty());
        mirror.history_cleared();
        assert!(mirror.log.is_empty());
    }

    #[test]
    fn a_snapshot_rebuilds_the_queue_and_keeps_the_newest_history_first() {
        let mut mirror = AttentionMirror::default();
        mirror.raised(&notice(9, AttentionKind::Error));
        mirror.replace(AttentionSnapshot {
            waiting: vec![AttentionWaiting {
                target: target(2),
                kind: AttentionKind::Ask,
                since: Timestamp::now(),
            }],
            history: vec![
                notice(2, AttentionKind::Ask),
                notice(3, AttentionKind::Finished),
            ],
        });
        assert!(!mirror.queue.contains(&target(9)));
        assert_eq!(mirror.queue.kind_of(&target(2)), Some(AttentionKind::Ask));
        let titles: Vec<&str> = mirror
            .log
            .filtered(InboxFilter::All)
            .map(|record| record.title.as_str())
            .collect();
        assert_eq!(titles, ["agent 2", "agent 3"]);
    }
}
