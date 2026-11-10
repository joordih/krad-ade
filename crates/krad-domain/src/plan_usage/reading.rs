use super::{LimitAlert, PlanUsage, ResetTime};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageFailure {
    SignedOut,
    NoSubscription,
    RateLimited { retry_at: Option<ResetTime> },
    Unreachable,
    Unrecognised,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageReading {
    Fresh(PlanUsage),
    Failed {
        last: Option<PlanUsage>,
        reason: UsageFailure,
    },
}

impl UsageReading {
    #[must_use]
    pub const fn usage(&self) -> Option<&PlanUsage> {
        match self {
            Self::Fresh(usage)
            | Self::Failed {
                last: Some(usage), ..
            } => Some(usage),
            Self::Failed { last: None, .. } => None,
        }
    }

    #[must_use]
    pub const fn failure(&self) -> Option<UsageFailure> {
        match self {
            Self::Fresh(_) => None,
            Self::Failed { reason, .. } => Some(*reason),
        }
    }

    #[must_use]
    pub fn after(
        previous: Option<Self>,
        read: Result<PlanUsage, UsageFailure>,
        now: SystemTime,
    ) -> Self {
        let last = previous.and_then(Self::into_usage);
        match read {
            Ok(mut usage) => {
                if usage.alert.is_none() {
                    usage.alert = last.and_then(|last| last.live_alert(now));
                }
                Self::Fresh(usage)
            }
            Err(reason) => Self::Failed { last, reason },
        }
    }

    #[must_use]
    pub fn merged(previous: Option<Self>, heard: PlanUsage) -> Self {
        let mut usage = previous.and_then(Self::into_usage).unwrap_or_default();
        if heard.plan.is_some() {
            usage.plan = heard.plan;
        }
        if heard.alert.is_some() {
            usage.alert = heard.alert;
        }
        for window in heard.windows {
            usage.place(window);
        }
        Self::Fresh(usage)
    }

    #[must_use]
    pub fn alerted(previous: Option<Self>, alert: Option<LimitAlert>) -> Option<Self> {
        let mut reading = previous?;
        match &mut reading {
            Self::Fresh(usage)
            | Self::Failed {
                last: Some(usage), ..
            } => usage.alert = alert,
            Self::Failed { last: None, .. } => {}
        }
        Some(reading)
    }

    fn into_usage(self) -> Option<PlanUsage> {
        match self {
            Self::Fresh(usage) => Some(usage),
            Self::Failed { last, .. } => last,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LimitStatus, UsageWindow, WindowSpan, UNTIMED_ALERT_LASTS};
    use super::*;
    use std::time::Duration;

    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    const RESET_SECS: u64 = 600;

    fn usage(used: u16) -> PlanUsage {
        PlanUsage {
            plan: Some("max".to_owned()),
            windows: vec![UsageWindow {
                span: WindowSpan::FiveHours,
                scope: None,
                used,
                resets_at: None,
                limited: false,
            }],
            alert: None,
        }
    }

    #[test]
    fn a_failed_read_keeps_the_last_good_usage_through_further_failures() {
        let fresh = UsageReading::after(None, Ok(usage(20)), NOW);
        assert_eq!(fresh, UsageReading::Fresh(usage(20)));
        let failed = UsageReading::after(Some(fresh), Err(UsageFailure::Unreachable), NOW);
        assert_eq!(failed.usage(), Some(&usage(20)));
        assert_eq!(failed.failure(), Some(UsageFailure::Unreachable));
        let again = UsageReading::after(Some(failed), Err(UsageFailure::SignedOut), NOW);
        assert_eq!(
            again,
            UsageReading::Failed {
                last: Some(usage(20)),
                reason: UsageFailure::SignedOut,
            }
        );
        let back = UsageReading::after(Some(again), Ok(usage(30)), NOW);
        assert_eq!(back, UsageReading::Fresh(usage(30)));
    }

    #[test]
    fn a_first_failed_read_has_nothing_to_show() {
        let failed = UsageReading::after(None, Err(UsageFailure::NoSubscription), NOW);
        assert_eq!(failed.usage(), None);
        assert_eq!(failed.failure(), Some(UsageFailure::NoSubscription));
    }

    #[test]
    fn heard_windows_merge_into_the_last_usage_and_alerts_survive_a_read() {
        let alert = LimitAlert {
            status: LimitStatus::Warning,
            resets_at: None,
            heard_at: NOW,
        };
        let kept = UsageReading::alerted(Some(UsageReading::Fresh(usage(20))), Some(alert));
        let read = UsageReading::after(kept.clone(), Ok(usage(25)), NOW);
        assert_eq!(read.usage().and_then(|usage| usage.alert), Some(alert));
        let later = UsageReading::after(kept, Ok(usage(25)), NOW + UNTIMED_ALERT_LASTS);
        assert_eq!(later.usage().and_then(|usage| usage.alert), None);
        let reset = LimitAlert {
            resets_at: Some(ResetTime::from_epoch_secs(RESET_SECS)),
            ..alert
        };
        let reached = UsageReading::alerted(Some(UsageReading::Fresh(usage(20))), Some(reset));
        let after_reset = NOW + Duration::from_secs(RESET_SECS);
        let reread = UsageReading::after(reached, Ok(usage(5)), after_reset);
        assert_eq!(reread.usage().and_then(|usage| usage.alert), None);
        let weekly = PlanUsage {
            windows: vec![UsageWindow {
                span: WindowSpan::Weekly,
                scope: None,
                used: 60,
                resets_at: None,
                limited: false,
            }],
            ..PlanUsage::default()
        };
        let merged = UsageReading::merged(Some(read), weekly);
        let shown = merged
            .usage()
            .map(|usage| (usage.windows.len(), usage.plan.clone()));
        assert_eq!(shown, Some((2, Some("max".to_owned()))));
        assert_eq!(UsageReading::alerted(None, Some(alert)), None);
    }
}
