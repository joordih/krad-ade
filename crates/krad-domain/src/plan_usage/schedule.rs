use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use super::UsageFailure;
use crate::AgentPreset;

pub const USAGE_REFRESH_INTERVAL: Duration = Duration::from_mins(5);
const USAGE_BACKOFF_BASE: Duration = Duration::from_mins(5);
const USAGE_BACKOFF_CAP: Duration = Duration::from_hours(1);
const SHARED_BUDGET_GAP: Duration = Duration::from_mins(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageCadence {
    Standard,
    SharedBudget,
}

impl UsageCadence {
    #[must_use]
    pub const fn of(harness: AgentPreset) -> Self {
        match harness {
            AgentPreset::ClaudeCode => Self::SharedBudget,
            AgentPreset::Codex
            | AgentPreset::Opencode
            | AgentPreset::Gateway
            | AgentPreset::Shell
            | AgentPreset::Acp(_) => Self::Standard,
        }
    }

    const fn min_gap(self) -> Duration {
        match self {
            Self::Standard => Duration::ZERO,
            Self::SharedBudget => SHARED_BUDGET_GAP,
        }
    }
}

#[derive(Debug, Default)]
pub struct UsageBudget {
    last_spent: Mutex<Option<Instant>>,
}

impl UsageBudget {
    #[must_use]
    pub fn try_spend(&self, now: Instant, cadence: UsageCadence) -> bool {
        let mut last = self
            .last_spent
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let open = last.is_none_or(|last| now >= last + cadence.min_gap());
        if open {
            *last = Some(now);
        }
        open
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageTrigger {
    Timer,
    Refresh,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageSchedule {
    next_at: Option<Instant>,
    held_until: Option<Instant>,
    last_read: Option<Instant>,
    failures: u32,
    reading: bool,
}

fn backoff(failures: u32) -> Duration {
    let factor = failures
        .checked_sub(1)
        .and_then(|doublings| 1_u32.checked_shl(doublings))
        .unwrap_or(u32::MAX);
    USAGE_BACKOFF_BASE
        .saturating_mul(factor)
        .min(USAGE_BACKOFF_CAP)
}

impl UsageSchedule {
    #[must_use]
    pub const fn is_reading(&self) -> bool {
        self.reading
    }

    #[must_use]
    pub fn due(&self, now: Instant, cadence: UsageCadence, ask: UsageTrigger) -> bool {
        if self.reading {
            return false;
        }
        let gap_open = self
            .last_read
            .is_none_or(|last| now >= last + cadence.min_gap());
        let unheld = self.held_until.is_none_or(|until| now >= until);
        let on_time = match ask {
            UsageTrigger::Refresh => true,
            UsageTrigger::Timer => self.next_at.is_none_or(|next| now >= next),
        };
        gap_open && unheld && on_time
    }

    pub const fn started(&mut self, now: Instant) {
        self.reading = true;
        self.last_read = Some(now);
    }

    pub fn finished(&mut self, now: Instant, wall: SystemTime, failure: Option<UsageFailure>) {
        self.reading = false;
        let Some(failure) = failure else {
            self.heard(now);
            return;
        };
        self.failures = self.failures.saturating_add(1);
        let backoff = backoff(self.failures);
        let lifts_in = match failure {
            UsageFailure::RateLimited { retry_at } => {
                Some(retry_at.map_or_default(|at| at.left(wall)))
            }
            _ => None,
        };
        let wait = lifts_in.map_or(backoff, |left| backoff.max(left));
        let until = now.checked_add(wait).unwrap_or(now + USAGE_BACKOFF_CAP);
        if lifts_in.is_some() {
            self.held_until = Some(until);
        }
        self.next_at = Some(until);
    }

    pub fn read_elsewhere(&mut self, now: Instant) {
        self.last_read = Some(now);
        self.heard(now);
    }

    pub fn heard(&mut self, now: Instant) {
        self.failures = 0;
        self.held_until = None;
        self.next_at = Some(now + USAGE_REFRESH_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::super::ResetTime;
    use super::*;

    const MINUTE: Duration = Duration::from_mins(1);

    fn wall() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    #[test]
    fn a_new_schedule_reads_at_once_and_then_every_interval() {
        let start = Instant::now();
        let mut schedule = UsageSchedule::default();
        let codex = UsageCadence::of(AgentPreset::Codex);
        assert!(schedule.due(start, codex, UsageTrigger::Timer));
        schedule.started(start);
        assert!(schedule.is_reading());
        assert!(!schedule.due(start, codex, UsageTrigger::Refresh));
        schedule.finished(start, wall(), None);
        assert!(!schedule.due(start + 4 * MINUTE, codex, UsageTrigger::Timer));
        assert!(schedule.due(start + USAGE_REFRESH_INTERVAL, codex, UsageTrigger::Timer));
        assert!(schedule.due(start + MINUTE, codex, UsageTrigger::Refresh));
    }

    #[test]
    fn failures_back_off_from_five_minutes_doubling_to_an_hour() {
        assert_eq!(backoff(1), 5 * MINUTE);
        assert_eq!(backoff(2), 10 * MINUTE);
        assert_eq!(backoff(3), 20 * MINUTE);
        assert_eq!(backoff(4), 40 * MINUTE);
        assert_eq!(backoff(5), 60 * MINUTE);
        assert_eq!(backoff(40), 60 * MINUTE);
        let start = Instant::now();
        let codex = UsageCadence::Standard;
        let mut schedule = UsageSchedule::default();
        for _ in 0..2 {
            schedule.started(start);
            schedule.finished(start, wall(), Some(UsageFailure::Unreachable));
        }
        assert!(!schedule.due(start + 9 * MINUTE, codex, UsageTrigger::Timer));
        assert!(schedule.due(start + 10 * MINUTE, codex, UsageTrigger::Timer));
        assert!(schedule.due(start, codex, UsageTrigger::Refresh));
        schedule.started(start);
        schedule.finished(start, wall(), None);
        assert!(schedule.due(start + 5 * MINUTE, codex, UsageTrigger::Timer));
    }

    #[test]
    fn a_rate_limit_holds_even_a_refresh_until_it_lifts() {
        let start = Instant::now();
        let mut schedule = UsageSchedule::default();
        let retry_at = Some(ResetTime::from_epoch_secs(1_000_000 + 30 * 60));
        schedule.started(start);
        schedule.finished(start, wall(), Some(UsageFailure::RateLimited { retry_at }));
        let codex = UsageCadence::Standard;
        assert!(!schedule.due(start + 29 * MINUTE, codex, UsageTrigger::Refresh));
        assert!(schedule.due(start + 30 * MINUTE, codex, UsageTrigger::Refresh));
        let mut unknown = UsageSchedule::default();
        unknown.started(start);
        let limited = UsageFailure::RateLimited { retry_at: None };
        unknown.finished(start, wall(), Some(limited));
        assert!(!unknown.due(start + 4 * MINUTE, codex, UsageTrigger::Refresh));
        assert!(unknown.due(start + 5 * MINUTE, codex, UsageTrigger::Refresh));
        let mut far = UsageSchedule::default();
        far.started(start);
        let never = UsageFailure::RateLimited {
            retry_at: Some(ResetTime::from_epoch_secs(u64::MAX)),
        };
        far.finished(start, wall(), Some(never));
        assert!(!far.due(start + 30 * MINUTE, codex, UsageTrigger::Refresh));
    }

    #[test]
    fn claude_reads_keep_a_minimum_gap_that_a_refresh_does_not_skip() {
        let start = Instant::now();
        let claude = UsageCadence::of(AgentPreset::ClaudeCode);
        let mut schedule = UsageSchedule::default();
        schedule.started(start);
        schedule.finished(start, wall(), Some(UsageFailure::Unreachable));
        assert!(!schedule.due(start + 4 * MINUTE, claude, UsageTrigger::Refresh));
        assert!(schedule.due(start + 5 * MINUTE, claude, UsageTrigger::Refresh));
        assert!(schedule.due(
            start + MINUTE,
            UsageCadence::Standard,
            UsageTrigger::Refresh
        ));
    }

    #[test]
    fn usage_heard_from_an_open_chat_resets_the_timer_and_the_backoff() {
        let start = Instant::now();
        let codex = UsageCadence::Standard;
        let mut schedule = UsageSchedule::default();
        schedule.started(start);
        schedule.finished(
            start,
            wall(),
            Some(UsageFailure::RateLimited { retry_at: None }),
        );
        schedule.heard(start + 2 * MINUTE);
        assert!(schedule.due(start + 2 * MINUTE, codex, UsageTrigger::Refresh));
        assert!(!schedule.due(start + 6 * MINUTE, codex, UsageTrigger::Timer));
        assert!(schedule.due(start + 7 * MINUTE, codex, UsageTrigger::Timer));
    }

    #[test]
    fn a_shared_budget_spends_at_most_once_per_gap() {
        let start = Instant::now();
        let budget = UsageBudget::default();
        let claude = UsageCadence::SharedBudget;
        assert!(budget.try_spend(start, claude));
        assert!(!budget.try_spend(start + MINUTE, claude));
        assert!(!budget.try_spend(start + 4 * MINUTE, claude));
        assert!(budget.try_spend(start + SHARED_BUDGET_GAP, claude));
        assert!(!budget.try_spend(start + SHARED_BUDGET_GAP + MINUTE, claude));
        let standard = UsageBudget::default();
        assert!(standard.try_spend(start, UsageCadence::Standard));
        assert!(standard.try_spend(start, UsageCadence::Standard));
    }

    #[test]
    fn a_read_by_an_open_chat_counts_against_the_claude_gap() {
        let start = Instant::now();
        let claude = UsageCadence::SharedBudget;
        let mut schedule = UsageSchedule::default();
        schedule.read_elsewhere(start);
        assert!(!schedule.due(start + 4 * MINUTE, claude, UsageTrigger::Refresh));
        assert!(schedule.due(start + SHARED_BUDGET_GAP, claude, UsageTrigger::Refresh));
    }
}
