use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

const MILLIS_PER_SECOND: u64 = 1_000;
const EPOCH_MILLIS_FROM: u64 = 10_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResetTime(u64);

impl ResetTime {
    #[must_use]
    pub const fn from_epoch_secs(secs: u64) -> Self {
        Self(secs)
    }

    #[must_use]
    pub const fn from_epoch_millis(millis: u64) -> Self {
        Self(millis / MILLIS_PER_SECOND)
    }

    #[must_use]
    pub const fn from_epoch(value: u64) -> Self {
        if value < EPOCH_MILLIS_FROM {
            Self::from_epoch_secs(value)
        } else {
            Self::from_epoch_millis(value)
        }
    }

    #[must_use]
    pub fn parse_iso(text: &str) -> Option<Self> {
        let at = DateTime::parse_from_rfc3339(text.trim()).ok()?;
        u64::try_from(at.timestamp()).ok().map(Self)
    }

    #[must_use]
    pub fn left(self, now: SystemTime) -> Duration {
        let elapsed = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        Duration::from_secs(self.0).saturating_sub(elapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_iso_time_in_utc_or_with_an_offset_reads_as_the_same_instant() {
        let utc = ResetTime::parse_iso("2026-11-09T18:30:00Z");
        assert_eq!(utc, Some(ResetTime::from_epoch_secs(1_794_249_000)));
        assert_eq!(
            ResetTime::parse_iso("2026-11-09T19:30:00.123456+01:00"),
            utc
        );
        assert_eq!(ResetTime::parse_iso("2026-11-09T18:30:00+00:00"), utc);
        assert_eq!(ResetTime::parse_iso("2026-11-09T13:30:00-05:00"), utc);
        assert_eq!(ResetTime::parse_iso(" 2026-11-09T18:30:00.5Z\n"), utc);
        assert_eq!(
            ResetTime::parse_iso("1970-01-01T00:00:00Z"),
            Some(ResetTime(0))
        );
        assert_eq!(
            ResetTime::parse_iso("2024-02-29T00:00:00Z"),
            Some(ResetTime(1_709_164_800))
        );
    }

    #[test]
    fn text_that_is_not_a_time_reads_as_none() {
        for text in [
            "",
            "soon",
            "2026-13-01T00:00:00Z",
            "2026-11-09",
            "2026-11-09T25:00:00Z",
            "2026-11-09T18:30:00",
            "1969-12-31T23:59:59Z",
        ] {
            assert_eq!(ResetTime::parse_iso(text), None, "{text}");
        }
    }

    #[test]
    fn what_is_left_until_a_reset_never_goes_below_zero() {
        let reset = ResetTime::from_epoch_secs(100);
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(40);
        assert_eq!(reset.left(now), Duration::from_secs(60));
        let later = SystemTime::UNIX_EPOCH + Duration::from_secs(400);
        assert_eq!(reset.left(later), Duration::ZERO);
        assert_eq!(ResetTime::from_epoch_millis(5_500), ResetTime(5));
        assert_eq!(
            ResetTime(u64::MAX).left(now),
            Duration::from_secs(u64::MAX - 40)
        );
    }

    #[test]
    fn an_epoch_reads_as_seconds_below_ten_digits_and_as_milliseconds_above() {
        assert_eq!(
            ResetTime::from_epoch(1_794_249_000),
            ResetTime(1_794_249_000)
        );
        assert_eq!(
            ResetTime::from_epoch(1_794_249_000_500),
            ResetTime(1_794_249_000)
        );
        assert_eq!(
            ResetTime::from_epoch(9_999_999_999),
            ResetTime(9_999_999_999)
        );
    }
}
