use super::*;
use chrono::{FixedOffset, Timelike, Utc};

const HOUR: i32 = 3600;

#[derive(Clone, Copy, Debug)]
pub(in crate::kraddie) struct Europe;

fn winter() -> FixedOffset {
    FixedOffset::east_opt(HOUR).unwrap_or_else(|| unreachable!())
}

fn summer() -> FixedOffset {
    FixedOffset::east_opt(2 * HOUR).unwrap_or_else(|| unreachable!())
}

pub(in crate::kraddie) fn utc(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap_or_else(|_| unreachable!())
}

impl TimeZone for Europe {
    type Offset = FixedOffset;

    fn from_offset(_: &FixedOffset) -> Self {
        Self
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<FixedOffset> {
        self.offset_from_local_datetime(
            &local
                .and_hms_opt(12, 0, 0)
                .unwrap_or_else(|| unreachable!()),
        )
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<FixedOffset> {
        let fits =
            |offset: FixedOffset| self.offset_from_utc_datetime(&(*local - offset)) == offset;
        match (fits(summer()), fits(winter())) {
            (true, true) => LocalResult::Ambiguous(summer(), winter()),
            (true, false) => LocalResult::Single(summer()),
            (false, true) => LocalResult::Single(winter()),
            (false, false) => LocalResult::None,
        }
    }

    fn offset_from_utc_date(&self, utc_date: &NaiveDate) -> FixedOffset {
        self.offset_from_utc_datetime(
            &utc_date
                .and_hms_opt(0, 0, 0)
                .unwrap_or_else(|| unreachable!()),
        )
    }

    fn offset_from_utc_datetime(&self, at: &NaiveDateTime) -> FixedOffset {
        if *at >= utc("2026-03-29 01:00") && *at < utc("2026-10-25 01:00") {
            summer()
        } else {
            winter()
        }
    }
}

pub(in crate::kraddie) fn local(text: &str) -> DateTime<Europe> {
    Europe
        .from_local_datetime(&utc(text))
        .earliest()
        .unwrap_or_else(|| unreachable!())
}

fn at(hour: u8, minute: u8, days: Weekdays) -> KraddieSchedule {
    KraddieSchedule::At {
        time: TimeOfDay::new(hour, minute).unwrap_or_else(|_| unreachable!()),
        days,
    }
}

#[test]
fn an_interval_runs_that_long_after_the_last_run() {
    let schedule = KraddieSchedule::Every {
        minutes: IntervalMinutes::try_from(90).unwrap_or_else(|_| unreachable!()),
    };
    let since = Utc.with_ymd_and_hms(2026, 11, 9, 10, 0, 0).unwrap();
    let next = schedule
        .next_after(&since)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(next, Utc.with_ymd_and_hms(2026, 11, 9, 11, 30, 0).unwrap());
    assert!(!schedule.is_due(&since, &(next - TimeDelta::seconds(1))));
    assert!(schedule.is_due(&since, &next));
    assert_eq!(IntervalMinutes::try_from(0), Err(IntervalError));
    assert_eq!(
        IntervalMinutes::try_from(MAX_INTERVAL_MINUTES + 1),
        Err(IntervalError)
    );
}

#[test]
fn a_time_of_day_waits_for_the_next_chosen_weekday() {
    let schedule = at(9, 0, Weekdays::WORKDAYS);
    let friday_noon = Utc.with_ymd_and_hms(2026, 11, 13, 12, 0, 0).unwrap();
    let next = schedule
        .next_after(&friday_noon)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(next, Utc.with_ymd_and_hms(2026, 11, 16, 9, 0, 0).unwrap());
    assert_eq!(next.weekday(), Weekday::Mon);
    let monday_early = Utc.with_ymd_and_hms(2026, 11, 16, 8, 59, 0).unwrap();
    assert_eq!(schedule.next_after(&monday_early), Some(next));
    assert_eq!(
        schedule.next_after(&next),
        Some(Utc.with_ymd_and_hms(2026, 11, 17, 9, 0, 0).unwrap())
    );
}

#[test]
fn a_time_lost_to_spring_forward_runs_at_the_first_valid_minute() {
    let schedule = at(2, 30, Weekdays::EVERY_DAY);
    let next = schedule
        .next_after(&local("2026-03-28 23:00"))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(next.naive_local(), utc("2026-03-29 03:00"));
    assert_eq!(next.naive_utc(), utc("2026-03-29 01:00"));
}

#[test]
fn a_time_repeated_by_fall_back_runs_once() {
    let schedule = at(2, 30, Weekdays::EVERY_DAY);
    let first = schedule
        .next_after(&local("2026-10-24 23:00"))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(first.naive_utc(), utc("2026-10-25 00:30"));
    let second = schedule
        .next_after(&first)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(second.naive_local(), utc("2026-10-26 02:30"));
    assert_eq!(second.hour(), 2);
}

#[test]
fn an_interval_across_spring_forward_keeps_its_real_length() {
    let schedule = KraddieSchedule::Every {
        minutes: IntervalMinutes::try_from(60).unwrap_or_else(|_| unreachable!()),
    };
    let next = schedule
        .next_after(&local("2026-03-29 01:30"))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(next.naive_local(), utc("2026-03-29 03:30"));
}

#[test]
fn weekdays_need_one_day_and_serialize_by_short_name() {
    assert_eq!(Weekdays::new(&[]), Err(WeekdaysError::Empty));
    let days = Weekdays::new(&[Weekday::Sat, Weekday::Mon]).unwrap_or_else(|_| unreachable!());
    assert_eq!(
        serde_json::to_string(&days).unwrap_or_else(|_| unreachable!()),
        r#"["mon","sat"]"#
    );
    assert_eq!(
        serde_json::from_str::<Weekdays>(r#"["mon","sat"]"#).unwrap_or_else(|_| unreachable!()),
        days
    );
    assert!(serde_json::from_str::<Weekdays>(r#"["someday"]"#).is_err());
    assert!(serde_json::from_str::<Weekdays>("[]").is_err());
    let only_monday = days.toggled(Weekday::Sat).unwrap_or_else(|| unreachable!());
    assert_eq!(only_monday.toggled(Weekday::Mon), None);
    let schedule = at(7, 5, days);
    let line = serde_json::to_string(&schedule).unwrap_or_else(|_| unreachable!());
    assert_eq!(line, r#"{"kind":"at","time":"07:05","days":["mon","sat"]}"#);
    assert_eq!(
        serde_json::from_str::<KraddieSchedule>(&line).unwrap_or_else(|_| unreachable!()),
        schedule
    );
}
