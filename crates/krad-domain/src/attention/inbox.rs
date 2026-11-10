use super::queue::{AttentionKind, AttentionTarget};
use std::collections::VecDeque;
use std::time::Instant;

pub const ATTENTION_HISTORY_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InboxFilter {
    #[default]
    All,
    Only(AttentionKind),
}

impl InboxFilter {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Only(AttentionKind::Finished),
        Self::Only(AttentionKind::Ask),
        Self::Only(AttentionKind::Permission),
        Self::Only(AttentionKind::Error),
    ];

    #[must_use]
    pub fn admits(self, kind: AttentionKind) -> bool {
        match self {
            Self::All => true,
            Self::Only(only) => only == kind,
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Only(kind) => kind.display_name(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttentionRecord {
    pub target: AttentionTarget,
    pub kind: AttentionKind,
    pub title: String,
    pub at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttentionLog {
    records: VecDeque<AttentionRecord>,
    limit: usize,
}

impl Default for AttentionLog {
    fn default() -> Self {
        Self::with_limit(ATTENTION_HISTORY_LIMIT)
    }
}

impl AttentionLog {
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            records: VecDeque::new(),
            limit,
        }
    }

    pub fn record(&mut self, record: AttentionRecord) {
        self.records.push_front(record);
        self.records.truncate(self.limit);
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn filtered(&self, filter: InboxFilter) -> impl Iterator<Item = &AttentionRecord> {
        self.records
            .iter()
            .filter(move |record| filter.admits(record.kind))
    }

    #[must_use]
    pub fn count(&self, filter: InboxFilter) -> usize {
        self.filtered(filter).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{EnvironmentName, TerminalId};

    fn record(id: u64, kind: AttentionKind) -> AttentionRecord {
        AttentionRecord {
            target: AttentionTarget::new(EnvironmentName::new("main".to_owned()), TerminalId(id)),
            kind,
            title: format!("agent {id}"),
            at: Instant::now(),
        }
    }

    fn ids(log: &AttentionLog, filter: InboxFilter) -> Vec<u64> {
        log.filtered(filter)
            .map(|record| record.target.terminal.as_u64())
            .collect()
    }

    #[test]
    fn filters_admit_every_kind_or_only_their_own() {
        for kind in AttentionKind::ALL {
            assert!(InboxFilter::All.admits(kind));
            for other in AttentionKind::ALL {
                assert_eq!(InboxFilter::Only(kind).admits(other), kind == other);
            }
        }
    }

    #[test]
    fn every_kind_has_its_own_filter_and_label() {
        for kind in AttentionKind::ALL {
            assert!(InboxFilter::ALL.contains(&InboxFilter::Only(kind)));
        }
        let labels: std::collections::HashSet<&str> = InboxFilter::ALL
            .iter()
            .map(|filter| filter.display_name())
            .collect();
        assert_eq!(labels.len(), InboxFilter::ALL.len());
    }

    #[test]
    fn history_lists_newest_first_and_filters_by_kind() {
        let mut log = AttentionLog::default();
        log.record(record(1, AttentionKind::Finished));
        log.record(record(2, AttentionKind::Permission));
        log.record(record(3, AttentionKind::Finished));
        assert_eq!(ids(&log, InboxFilter::All), [3, 2, 1]);
        assert_eq!(
            ids(&log, InboxFilter::Only(AttentionKind::Finished)),
            [3, 1]
        );
        assert_eq!(log.count(InboxFilter::Only(AttentionKind::Permission)), 1);
        assert_eq!(log.count(InboxFilter::Only(AttentionKind::Error)), 0);
    }

    #[test]
    fn history_keeps_only_its_newest_records() {
        let mut log = AttentionLog::with_limit(2);
        for id in 1..=3 {
            log.record(record(id, AttentionKind::Ask));
        }
        assert_eq!(ids(&log, InboxFilter::All), [3, 2]);
        log.clear();
        assert!(log.is_empty());
    }
}
