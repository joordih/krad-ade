use crate::attention::TimeOfDay;
use chrono::{DateTime, FixedOffset, NaiveDate, Offset as _, Timelike, Utc};
use std::time::SystemTime;

const SECONDS_PER_DAY: i32 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalOffset(FixedOffset);

impl LocalOffset {
    #[must_use]
    pub fn east_seconds(seconds: i32) -> Option<Self> {
        (seconds.abs() < SECONDS_PER_DAY)
            .then(|| FixedOffset::east_opt(seconds))
            .flatten()
            .map(Self)
    }

    #[must_use]
    pub fn zoned(self, at: SystemTime) -> DateTime<FixedOffset> {
        DateTime::<Utc>::from(at).with_timezone(&self.0)
    }
}

impl Default for LocalOffset {
    fn default() -> Self {
        Self(Utc.fix())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalMoment {
    pub at: SystemTime,
    pub offset: LocalOffset,
}

impl LocalMoment {
    #[must_use]
    pub fn utc(at: SystemTime) -> Self {
        Self {
            at,
            offset: LocalOffset::default(),
        }
    }

    #[must_use]
    pub fn zoned(self) -> DateTime<FixedOffset> {
        self.offset.zoned(self.at)
    }

    #[must_use]
    pub fn date(self) -> NaiveDate {
        self.zoned().date_naive()
    }

    #[must_use]
    pub fn time_of_day(self) -> Option<TimeOfDay> {
        let zoned = self.zoned();
        TimeOfDay::new(
            u8::try_from(zoned.hour()).ok()?,
            u8::try_from(zoned.minute()).ok()?,
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_moment_reads_its_time_of_day_in_its_own_offset() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_mins(6 * 60 + 5);
        let utc = LocalMoment::utc(at);
        assert_eq!(utc.time_of_day(), TimeOfDay::new(6, 5).ok());
        let east = LocalMoment {
            at,
            offset: LocalOffset::east_seconds(2 * 3600).unwrap_or_default(),
        };
        assert_eq!(east.time_of_day(), TimeOfDay::new(8, 5).ok());
        assert_eq!(LocalOffset::east_seconds(SECONDS_PER_DAY), None);
    }
}
