use super::moment::LocalOffset;
use super::trigger::{code_host_name, Trigger};
use crate::kraddie::schedule::{KraddieSchedule, Weekdays};
use std::time::SystemTime;

pub const MANUAL_LABEL: &str = "Manual or krad CLI";
const RUN_FORMAT: &str = "%a %d %b %H:%M";
const EVERY_LABEL: &str = "Every";
const MINUTES_LABEL: &str = "min";
const HOURS_LABEL: &str = "h";
const MINUTES_PER_HOUR: u32 = 60;
const EVERY_DAY_LABEL: &str = "every day";
const WORKDAYS_LABEL: &str = "weekdays";
const DAY_SEPARATOR: &str = " ";
const HOST_SEPARATOR: &str = ": ";

#[must_use]
pub fn local_stamp(at: SystemTime, offset: LocalOffset) -> String {
    offset.zoned(at).to_rfc3339()
}

#[must_use]
pub fn run_stamp(at: SystemTime, offset: LocalOffset) -> String {
    offset.zoned(at).format(RUN_FORMAT).to_string()
}

#[must_use]
pub fn schedule_label(schedule: KraddieSchedule) -> String {
    match schedule {
        KraddieSchedule::Every { minutes } => {
            let minutes = minutes.get();
            if minutes % MINUTES_PER_HOUR == 0 {
                format!("{EVERY_LABEL} {} {HOURS_LABEL}", minutes / MINUTES_PER_HOUR)
            } else {
                format!("{EVERY_LABEL} {minutes} {MINUTES_LABEL}")
            }
        }
        KraddieSchedule::At { time, days } => {
            let days = match days {
                Weekdays::EVERY_DAY => EVERY_DAY_LABEL.to_owned(),
                Weekdays::WORKDAYS => WORKDAYS_LABEL.to_owned(),
                days => days
                    .days()
                    .map(Weekdays::short_name)
                    .collect::<Vec<_>>()
                    .join(DAY_SEPARATOR),
            };
            format!("{time} {days}")
        }
    }
}

#[must_use]
pub fn trigger_label(trigger: Trigger) -> String {
    match trigger {
        Trigger::Schedule { schedule } => schedule_label(schedule),
        Trigger::App { event } => event.label().to_owned(),
        Trigger::CodeHost { host, event } => {
            format!(
                "{}{HOST_SEPARATOR}{}",
                code_host_name(host),
                event.label(host)
            )
        }
        Trigger::Manual => MANUAL_LABEL.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppTrigger, CodeHost, CodeHostTrigger, IntervalMinutes, TimeOfDay};

    #[test]
    fn schedules_and_triggers_read_as_short_labels() {
        let every = |minutes| KraddieSchedule::Every {
            minutes: IntervalMinutes::try_from(minutes).unwrap_or_else(|_| unreachable!()),
        };
        assert_eq!(schedule_label(every(45)), "Every 45 min");
        assert_eq!(schedule_label(every(120)), "Every 2 h");
        let time = TimeOfDay::new(6, 5).unwrap_or_else(|_| unreachable!());
        let workdays = KraddieSchedule::At {
            time,
            days: Weekdays::WORKDAYS,
        };
        assert_eq!(schedule_label(workdays), "06:05 weekdays");
        let weekend = KraddieSchedule::At {
            time,
            days: Weekdays::new(&[chrono::Weekday::Sat, chrono::Weekday::Sun])
                .unwrap_or_else(|_| unreachable!()),
        };
        assert_eq!(schedule_label(weekend), "06:05 sat sun");
        assert_eq!(
            trigger_label(Trigger::CodeHost {
                host: CodeHost::Gitlab,
                event: CodeHostTrigger::CiFailed,
            }),
            "GitLab: Pipeline failed"
        );
        assert_eq!(
            trigger_label(Trigger::App {
                event: AppTrigger::GitChanged
            }),
            "Git changes"
        );
        assert_eq!(trigger_label(Trigger::Manual), MANUAL_LABEL);
    }
}
