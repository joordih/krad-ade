use super::rules::Switch;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const MINUTES_PER_HOUR: u16 = 60;
const HOURS_PER_DAY: u16 = 24;
const MINUTES_PER_DAY: u16 = MINUTES_PER_HOUR * HOURS_PER_DAY;
const CLOCK_SEPARATOR: char = ':';
const DEFAULT_QUIET_START_HOUR: u8 = 22;
const DEFAULT_QUIET_END_HOUR: u8 = 8;

pub const DEFAULT_QUIET_START: TimeOfDay = TimeOfDay::at_hour(DEFAULT_QUIET_START_HOUR);
pub const DEFAULT_QUIET_END: TimeOfDay = TimeOfDay::at_hour(DEFAULT_QUIET_END_HOUR);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TimeOfDay {
    minutes: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTimeOfDay;

impl fmt::Display for InvalidTimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a time of day")
    }
}

impl std::error::Error for InvalidTimeOfDay {}

impl TimeOfDay {
    const fn at_hour(hour: u8) -> Self {
        Self {
            minutes: hour as u16 * MINUTES_PER_HOUR,
        }
    }

    /// # Errors
    /// Fails when the hour or minute is beyond what a clock shows.
    pub fn new(hour: u8, minute: u8) -> Result<Self, InvalidTimeOfDay> {
        let (hour, minute) = (u16::from(hour), u16::from(minute));
        if hour >= HOURS_PER_DAY || minute >= MINUTES_PER_HOUR {
            return Err(InvalidTimeOfDay);
        }
        Ok(Self {
            minutes: hour * MINUTES_PER_HOUR + minute,
        })
    }

    #[must_use]
    pub const fn hour(self) -> u16 {
        self.minutes / MINUTES_PER_HOUR
    }

    #[must_use]
    pub const fn minute(self) -> u16 {
        self.minutes % MINUTES_PER_HOUR
    }

    #[must_use]
    pub fn shifted(self, minutes: i32) -> Self {
        let day = i32::from(MINUTES_PER_DAY);
        let shifted = (i32::from(self.minutes) + minutes).rem_euclid(day);
        Self {
            minutes: u16::try_from(shifted).unwrap_or_default(),
        }
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}{CLOCK_SEPARATOR}{:02}", self.hour(), self.minute())
    }
}

impl FromStr for TimeOfDay {
    type Err = InvalidTimeOfDay;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (hour, minute) = text.split_once(CLOCK_SEPARATOR).ok_or(InvalidTimeOfDay)?;
        let hour = hour.trim().parse().map_err(|_| InvalidTimeOfDay)?;
        let minute = minute.trim().parse().map_err(|_| InvalidTimeOfDay)?;
        Self::new(hour, minute)
    }
}

impl TryFrom<String> for TimeOfDay {
    type Error = InvalidTimeOfDay;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<TimeOfDay> for String {
    fn from(time: TimeOfDay) -> Self {
        time.to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct QuietHours {
    pub switch: Switch,
    pub start: TimeOfDay,
    pub end: TimeOfDay,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            switch: Switch::Off,
            start: DEFAULT_QUIET_START,
            end: DEFAULT_QUIET_END,
        }
    }
}

impl QuietHours {
    #[must_use]
    pub fn contains(&self, now: Option<TimeOfDay>) -> bool {
        let Some(now) = now else {
            return false;
        };
        if !self.switch.is_on() {
            return false;
        }
        match self.start.cmp(&self.end) {
            std::cmp::Ordering::Less => self.start <= now && now < self.end,
            std::cmp::Ordering::Greater => now >= self.start || now < self.end,
            std::cmp::Ordering::Equal => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u8, minute: u8) -> TimeOfDay {
        TimeOfDay::new(hour, minute).unwrap_or(DEFAULT_QUIET_START)
    }

    fn quiet(start: TimeOfDay, end: TimeOfDay) -> QuietHours {
        QuietHours {
            switch: Switch::On,
            start,
            end,
        }
    }

    #[test]
    fn times_of_day_reject_what_a_clock_cannot_show() {
        assert_eq!(TimeOfDay::new(24, 0), Err(InvalidTimeOfDay));
        assert_eq!(TimeOfDay::new(7, 60), Err(InvalidTimeOfDay));
        assert_eq!("7".parse::<TimeOfDay>(), Err(InvalidTimeOfDay));
        assert_eq!("ab:cd".parse::<TimeOfDay>(), Err(InvalidTimeOfDay));
        assert_eq!("07:05".parse::<TimeOfDay>(), Ok(at(7, 5)));
        assert_eq!(at(7, 5).to_string(), "07:05");
    }

    #[test]
    fn shifting_wraps_around_midnight() {
        assert_eq!(at(23, 30).shifted(60), at(0, 30));
        assert_eq!(at(0, 15).shifted(-30), at(23, 45));
        assert_eq!(at(12, 0).shifted(0), at(12, 0));
    }

    #[test]
    fn quiet_hours_across_midnight_cover_the_night_only() {
        let night = quiet(at(22, 0), at(8, 0));
        assert!(night.contains(Some(at(22, 0))));
        assert!(night.contains(Some(at(23, 59))));
        assert!(night.contains(Some(at(3, 0))));
        assert!(!night.contains(Some(at(8, 0))));
        assert!(!night.contains(Some(at(12, 0))));
        assert!(!night.contains(Some(at(21, 59))));
    }

    #[test]
    fn quiet_hours_within_a_day_cover_their_span() {
        let lunch = quiet(at(13, 0), at(14, 30));
        assert!(lunch.contains(Some(at(13, 0))));
        assert!(lunch.contains(Some(at(14, 29))));
        assert!(!lunch.contains(Some(at(14, 30))));
        assert!(!lunch.contains(Some(at(12, 59))));
    }

    #[test]
    fn quiet_hours_are_never_on_when_off_empty_or_without_a_clock() {
        let night = quiet(at(22, 0), at(8, 0));
        let off = QuietHours {
            switch: Switch::Off,
            ..night
        };
        assert!(!off.contains(Some(at(23, 0))));
        assert!(!quiet(at(9, 0), at(9, 0)).contains(Some(at(9, 0))));
        assert!(!night.contains(None));
    }

    #[test]
    fn quiet_hours_default_to_off_from_ten_at_night_to_eight() {
        let defaults = QuietHours::default();
        assert_eq!(defaults.switch, Switch::Off);
        assert_eq!(defaults.start, at(22, 0));
        assert_eq!(defaults.end, at(8, 0));
    }

    #[test]
    fn quiet_hours_store_their_times_as_clock_text() -> Result<(), serde_json::Error> {
        let stored = serde_json::to_string(&quiet(at(22, 30), at(7, 0)))?;
        assert_eq!(stored, r#"{"switch":"on","start":"22:30","end":"07:00"}"#);
        let read: QuietHours = serde_json::from_str(r#"{"start":"25:00"}"#).unwrap_or_default();
        assert_eq!(read, QuietHours::default());
        Ok(())
    }
}
