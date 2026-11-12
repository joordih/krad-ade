use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

const DAY: Duration = Duration::from_hours(24);
const SEVEN_DAYS: Duration = DAY.saturating_mul(7);
const THIRTY_DAYS: Duration = DAY.saturating_mul(30);
const NINETY_DAYS: Duration = DAY.saturating_mul(90);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveRetention {
    Always,
    SevenDays,
    #[default]
    ThirtyDays,
    NinetyDays,
    NotAtAll,
}

impl ArchiveRetention {
    pub const ALL: [Self; 5] = [
        Self::Always,
        Self::SevenDays,
        Self::ThirtyDays,
        Self::NinetyDays,
        Self::NotAtAll,
    ];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::SevenDays => "7 days",
            Self::ThirtyDays => "30 days",
            Self::NinetyDays => "90 days",
            Self::NotAtAll => "Not at all",
        }
    }

    #[must_use]
    pub const fn keeps(self) -> bool {
        !matches!(self, Self::NotAtAll)
    }

    #[must_use]
    pub const fn limit(self) -> Option<Duration> {
        match self {
            Self::Always => None,
            Self::SevenDays => Some(SEVEN_DAYS),
            Self::ThirtyDays => Some(THIRTY_DAYS),
            Self::NinetyDays => Some(NINETY_DAYS),
            Self::NotAtAll => Some(Duration::ZERO),
        }
    }

    #[must_use]
    pub fn has_expired(self, closed: SystemTime, now: SystemTime) -> bool {
        let Some(limit) = self.limit() else {
            return false;
        };
        if !self.keeps() {
            return true;
        }
        now.duration_since(closed)
            .is_ok_and(|kept_for| kept_for >= limit)
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AN_HOUR: Duration = Duration::from_hours(1);

    fn closed_ago(ago: Duration) -> (SystemTime, SystemTime) {
        let now = SystemTime::UNIX_EPOCH + NINETY_DAYS + NINETY_DAYS;
        (now - ago, now)
    }

    #[test]
    fn each_retention_expires_what_it_kept_past_its_limit() {
        let cases = [
            (ArchiveRetention::SevenDays, SEVEN_DAYS),
            (ArchiveRetention::ThirtyDays, THIRTY_DAYS),
            (ArchiveRetention::NinetyDays, NINETY_DAYS),
        ];
        for (retention, limit) in cases {
            let (closed, now) = closed_ago(limit.saturating_sub(AN_HOUR));
            assert!(!retention.has_expired(closed, now), "{retention:?}");
            let (closed, now) = closed_ago(limit);
            assert!(retention.has_expired(closed, now), "{retention:?}");
        }
    }

    #[test]
    fn always_keeps_forever_and_not_at_all_keeps_nothing() {
        let (old, now) = closed_ago(NINETY_DAYS + NINETY_DAYS);
        assert!(!ArchiveRetention::Always.has_expired(old, now));
        assert!(ArchiveRetention::NotAtAll.has_expired(now, now));
        assert!(!ArchiveRetention::NotAtAll.keeps());
        assert!(ArchiveRetention::Always.keeps());
    }

    #[test]
    fn something_closed_after_now_has_not_expired() {
        let (_, now) = closed_ago(Duration::ZERO);
        let ahead = now + AN_HOUR;
        assert!(!ArchiveRetention::SevenDays.has_expired(ahead, now));
    }

    #[test]
    fn thirty_days_is_the_default_and_round_trips() -> serde_json::Result<()> {
        assert_eq!(ArchiveRetention::default(), ArchiveRetention::ThirtyDays);
        let json = serde_json::to_string(&ArchiveRetention::NotAtAll)?;
        assert_eq!(json, r#""not_at_all""#);
        assert_eq!(
            serde_json::from_str::<ArchiveRetention>(&json)?,
            ArchiveRetention::NotAtAll
        );
        Ok(())
    }
}
