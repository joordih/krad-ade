use crate::attention::TimeOfDay;
use chrono::{
    DateTime, Datelike, LocalResult, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Weekday,
};
use serde::{Deserialize, Serialize};
use std::fmt;

const MIN_INTERVAL_MINUTES: u32 = 1;
const MAX_INTERVAL_MINUTES: u32 = 7 * 24 * 60;
const DAYS_AHEAD: usize = 8;
const GAP_STEP: TimeDelta = TimeDelta::minutes(1);
const MAX_GAP_STEPS: u32 = 24 * 60;
const WEEK: [Weekday; 7] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];
const DAY_NAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntervalError;

impl fmt::Display for IntervalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("An interval is between one minute and one week")
    }
}

impl std::error::Error for IntervalError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct IntervalMinutes(u32);

impl IntervalMinutes {
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn delta(self) -> TimeDelta {
        TimeDelta::minutes(i64::from(self.0))
    }
}

impl TryFrom<u32> for IntervalMinutes {
    type Error = IntervalError;

    fn try_from(minutes: u32) -> Result<Self, Self::Error> {
        (MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES)
            .contains(&minutes)
            .then_some(Self(minutes))
            .ok_or(IntervalError)
    }
}

impl From<IntervalMinutes> for u32 {
    fn from(minutes: IntervalMinutes) -> Self {
        minutes.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeekdaysError {
    Empty,
    Unknown,
}

impl fmt::Display for WeekdaysError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "Pick at least one weekday",
            Self::Unknown => "Not a weekday",
        })
    }
}

impl std::error::Error for WeekdaysError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub struct Weekdays(u8);

impl Weekdays {
    pub const EVERY_DAY: Self = Self(0b111_1111);
    pub const WORKDAYS: Self = Self(0b001_1111);

    const fn bit(day: Weekday) -> u8 {
        1 << day.num_days_from_monday()
    }

    /// # Errors
    /// Fails when no day is given.
    pub fn new(days: &[Weekday]) -> Result<Self, WeekdaysError> {
        let bits = days.iter().fold(0, |bits, day| bits | Self::bit(*day));
        (bits != 0)
            .then_some(Self(bits))
            .ok_or(WeekdaysError::Empty)
    }

    #[must_use]
    pub const fn contains(self, day: Weekday) -> bool {
        self.0 & Self::bit(day) != 0
    }

    #[must_use]
    pub fn toggled(self, day: Weekday) -> Option<Self> {
        let bits = self.0 ^ Self::bit(day);
        (bits != 0).then_some(Self(bits))
    }

    pub fn days(self) -> impl Iterator<Item = Weekday> {
        WEEK.into_iter().filter(move |day| self.contains(*day))
    }

    #[must_use]
    pub const fn all() -> [Weekday; 7] {
        WEEK
    }

    #[must_use]
    pub fn short_name(day: Weekday) -> &'static str {
        DAY_NAMES[day.num_days_from_monday() as usize]
    }
}

impl TryFrom<Vec<String>> for Weekdays {
    type Error = WeekdaysError;

    fn try_from(names: Vec<String>) -> Result<Self, Self::Error> {
        let days = names
            .iter()
            .map(|name| {
                DAY_NAMES
                    .iter()
                    .position(|known| known.eq_ignore_ascii_case(name.trim()))
                    .map(|index| WEEK[index])
                    .ok_or(WeekdaysError::Unknown)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(&days)
    }
}

impl From<Weekdays> for Vec<String> {
    fn from(days: Weekdays) -> Self {
        days.days()
            .map(|day| Weekdays::short_name(day).to_owned())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KraddieSchedule {
    Every { minutes: IntervalMinutes },
    At { time: TimeOfDay, days: Weekdays },
}

impl KraddieSchedule {
    #[must_use]
    pub fn next_after<Tz: TimeZone>(&self, since: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        match self {
            Self::Every { minutes } => since.clone().checked_add_signed(minutes.delta()),
            Self::At { time, days } => next_slot(since, *time, *days),
        }
    }

    #[must_use]
    pub fn is_due<Tz: TimeZone>(&self, since: &DateTime<Tz>, now: &DateTime<Tz>) -> bool {
        self.next_after(since).is_some_and(|next| next <= *now)
    }
}

fn next_slot<Tz: TimeZone>(
    since: &DateTime<Tz>,
    time: TimeOfDay,
    days: Weekdays,
) -> Option<DateTime<Tz>> {
    let zone = since.timezone();
    let first = since.date_naive();
    first
        .iter_days()
        .take(DAYS_AHEAD)
        .filter(|date| days.contains(date.weekday()))
        .filter_map(|date| local_at(&zone, date, time))
        .find(|slot| slot > since)
}

fn local_at<Tz: TimeZone>(zone: &Tz, date: NaiveDate, time: TimeOfDay) -> Option<DateTime<Tz>> {
    let mut naive: NaiveDateTime =
        date.and_hms_opt(u32::from(time.hour()), u32::from(time.minute()), 0)?;
    for _ in 0..MAX_GAP_STEPS {
        match zone.from_local_datetime(&naive) {
            LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => return Some(at),
            LocalResult::None => naive = naive.checked_add_signed(GAP_STEP)?,
        }
    }
    None
}

#[cfg(test)]
pub(in crate::kraddie) mod tests;
