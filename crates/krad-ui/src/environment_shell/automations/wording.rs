use chrono::Weekday;
use krad_domain::{
    AppTrigger, CodeHost, CodeHostTrigger, KraddieOutcome, KraddieSchedule, Trigger, Weekdays,
};
use std::time::Duration;

const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);
const MINUTES_PER_HOUR: u32 = 60;
const SECONDS_PER_MINUTE: u64 = MINUTE.as_secs();
const SECONDS_PER_HOUR: u64 = HOUR.as_secs();
const SECONDS_PER_DAY: u64 = DAY.as_secs();
pub(super) const SECOND_TICK: Duration = Duration::from_secs(1);
pub(super) const MINUTE_TICK: Duration = MINUTE;
const DAILY: &str = "Daily";
const ON_WEEKDAYS: &str = "Weekdays";
const ON_WEEKENDS: &str = "Weekends";
const DAY_LIST_SEPARATOR: &str = ", ";
const MORE_CUES: &str = "more";
const MANUAL_CUE: &str = "Only when you ask";
const ONE_MINUTE: &str = "Every minute";
const ONE_HOUR: &str = "Every hour";
const JUST_NOW: &str = "just now";
const SOON: &str = "any moment";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Live,
    Good,
    Attention,
    Bad,
    Quiet,
}

#[must_use]
pub(super) const fn tone(outcome: Option<&KraddieOutcome>) -> Tone {
    match outcome {
        None | Some(KraddieOutcome::Refused { .. }) => Tone::Quiet,
        Some(KraddieOutcome::Starting | KraddieOutcome::Working) => Tone::Live,
        Some(KraddieOutcome::NeedsYou) => Tone::Attention,
        Some(KraddieOutcome::Finished) => Tone::Good,
        Some(KraddieOutcome::Failed { .. } | KraddieOutcome::Interrupted) => Tone::Bad,
    }
}

const fn plural_day(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Mondays",
        Weekday::Tue => "Tuesdays",
        Weekday::Wed => "Wednesdays",
        Weekday::Thu => "Thursdays",
        Weekday::Fri => "Fridays",
        Weekday::Sat => "Saturdays",
        Weekday::Sun => "Sundays",
    }
}

const fn short_day(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

fn day_phrase(days: Weekdays) -> String {
    if days == Weekdays::EVERY_DAY {
        return DAILY.to_owned();
    }
    if days == Weekdays::WORKDAYS {
        return ON_WEEKDAYS.to_owned();
    }
    if Weekdays::new(&[Weekday::Sat, Weekday::Sun]).is_ok_and(|weekend| weekend == days) {
        return ON_WEEKENDS.to_owned();
    }
    let mut listed = days.days();
    match (listed.next(), listed.next()) {
        (Some(only), None) => plural_day(only).to_owned(),
        _ => days
            .days()
            .map(short_day)
            .collect::<Vec<_>>()
            .join(DAY_LIST_SEPARATOR),
    }
}

#[must_use]
pub(super) fn schedule_sentence(schedule: KraddieSchedule) -> String {
    match schedule {
        KraddieSchedule::Every { minutes } => match minutes.get() {
            1 => ONE_MINUTE.to_owned(),
            MINUTES_PER_HOUR => ONE_HOUR.to_owned(),
            minutes if minutes % MINUTES_PER_HOUR == 0 => {
                format!("Every {} hours", minutes / MINUTES_PER_HOUR)
            }
            minutes => format!("Every {minutes} minutes"),
        },
        KraddieSchedule::At { time, days } => format!("{} at {time}", day_phrase(days)),
    }
}

#[must_use]
pub(super) const fn app_sentence(event: AppTrigger) -> &'static str {
    match event {
        AppTrigger::AgentFinished => "When an agent wraps up",
        AppTrigger::AgentWaiting => "When an agent is waiting on you",
        AppTrigger::GitChanged => "When files in the repository change",
        AppTrigger::WorktreeCreated => "When a worktree appears",
        AppTrigger::WorktreeRemoved => "When a worktree goes away",
        AppTrigger::VariantsFinished => "When a variants race ends",
    }
}

#[must_use]
pub(super) const fn code_host_sentence(host: CodeHost, event: CodeHostTrigger) -> &'static str {
    match (host, event) {
        (CodeHost::Github, CodeHostTrigger::RequestOpened) => "When a pull request opens",
        (CodeHost::Gitlab, CodeHostTrigger::RequestOpened) => "When a merge request opens",
        (CodeHost::Github, CodeHostTrigger::CiFailed) => "When GitHub checks turn red",
        (CodeHost::Gitlab, CodeHostTrigger::CiFailed) => "When a GitLab pipeline turns red",
        (CodeHost::Github, CodeHostTrigger::CiPassed) => "When GitHub checks turn green",
        (CodeHost::Gitlab, CodeHostTrigger::CiPassed) => "When a GitLab pipeline turns green",
    }
}

#[must_use]
pub(super) fn cue_sentence(trigger: Trigger) -> String {
    match trigger {
        Trigger::Schedule { schedule } => schedule_sentence(schedule),
        Trigger::App { event } => app_sentence(event).to_owned(),
        Trigger::CodeHost { host, event } => code_host_sentence(host, event).to_owned(),
        Trigger::Manual => MANUAL_CUE.to_owned(),
    }
}

#[must_use]
pub(super) fn cues_summary(triggers: &[Trigger]) -> String {
    let Some(first) = triggers.first() else {
        return MANUAL_CUE.to_owned();
    };
    let lead = cue_sentence(*first);
    match triggers.len() - 1 {
        0 => lead,
        others => format!("{lead} +{others} {MORE_CUES}"),
    }
}

#[must_use]
pub(super) fn countdown(remaining: Duration) -> String {
    let seconds = remaining.as_secs();
    if seconds == 0 {
        return SOON.to_owned();
    }
    if seconds < SECONDS_PER_MINUTE {
        return format!("in {seconds}s");
    }
    if seconds < SECONDS_PER_HOUR {
        return format!(
            "in {}m {:02}s",
            seconds / SECONDS_PER_MINUTE,
            seconds % SECONDS_PER_MINUTE
        );
    }
    if seconds < SECONDS_PER_DAY {
        return format!(
            "in {}h {:02}m",
            seconds / SECONDS_PER_HOUR,
            seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE
        );
    }
    format!(
        "in {}d {}h",
        seconds / SECONDS_PER_DAY,
        seconds % SECONDS_PER_DAY / SECONDS_PER_HOUR
    )
}

#[must_use]
pub(super) const fn countdown_tick(remaining: Duration) -> Duration {
    if remaining.as_secs() < SECONDS_PER_HOUR {
        SECOND_TICK
    } else {
        MINUTE_TICK
    }
}

#[must_use]
pub(super) fn elapsed(since: Duration) -> String {
    let seconds = since.as_secs();
    if seconds < SECONDS_PER_MINUTE {
        return JUST_NOW.to_owned();
    }
    if seconds < SECONDS_PER_HOUR {
        return format!("{}m ago", seconds / SECONDS_PER_MINUTE);
    }
    if seconds < SECONDS_PER_DAY {
        return format!("{}h ago", seconds / SECONDS_PER_HOUR);
    }
    format!("{}d ago", seconds / SECONDS_PER_DAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{IntervalMinutes, TimeOfDay};

    fn every(minutes: u32) -> KraddieSchedule {
        KraddieSchedule::Every {
            minutes: IntervalMinutes::try_from(minutes).unwrap_or_else(|_| unreachable!()),
        }
    }

    fn at(days: &[Weekday]) -> KraddieSchedule {
        KraddieSchedule::At {
            time: TimeOfDay::new(8, 30).unwrap_or_else(|_| unreachable!()),
            days: Weekdays::new(days).unwrap_or_else(|_| unreachable!()),
        }
    }

    #[test]
    fn schedules_read_as_plain_sentences() {
        assert_eq!(schedule_sentence(every(1)), "Every minute");
        assert_eq!(schedule_sentence(every(25)), "Every 25 minutes");
        assert_eq!(schedule_sentence(every(60)), "Every hour");
        assert_eq!(schedule_sentence(every(240)), "Every 4 hours");
        assert_eq!(schedule_sentence(every(90)), "Every 90 minutes");
        assert_eq!(
            schedule_sentence(at(&[
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri
            ])),
            "Weekdays at 08:30"
        );
        assert_eq!(
            schedule_sentence(at(&[Weekday::Sat, Weekday::Sun])),
            "Weekends at 08:30"
        );
        assert_eq!(schedule_sentence(at(&[Weekday::Fri])), "Fridays at 08:30");
        assert_eq!(
            schedule_sentence(at(&[Weekday::Mon, Weekday::Thu])),
            "Mon, Thu at 08:30"
        );
    }

    #[test]
    fn several_cues_lead_with_the_first_and_count_the_rest() {
        let ci = Trigger::CodeHost {
            host: CodeHost::Gitlab,
            event: CodeHostTrigger::CiFailed,
        };
        assert_eq!(cues_summary(&[]), MANUAL_CUE);
        assert_eq!(cues_summary(&[ci]), "When a GitLab pipeline turns red");
        assert_eq!(
            cues_summary(&[
                ci,
                Trigger::Manual,
                Trigger::App {
                    event: AppTrigger::GitChanged
                }
            ]),
            "When a GitLab pipeline turns red +2 more"
        );
    }

    #[test]
    fn countdowns_tick_by_seconds_only_within_the_hour() {
        assert_eq!(countdown(Duration::ZERO), SOON);
        assert_eq!(countdown(Duration::from_secs(42)), "in 42s");
        assert_eq!(countdown(Duration::from_secs(605)), "in 10m 05s");
        assert_eq!(
            countdown(Duration::from_hours(2) + Duration::from_mins(14)),
            "in 2h 14m"
        );
        assert_eq!(countdown(Duration::from_hours(3 * 24 + 2)), "in 3d 2h");
        assert_eq!(countdown_tick(Duration::from_mins(59)), SECOND_TICK);
        assert_eq!(countdown_tick(Duration::from_secs(3_600)), MINUTE_TICK);
    }

    #[test]
    fn past_runs_read_as_a_rough_age() {
        assert_eq!(elapsed(Duration::from_secs(5)), JUST_NOW);
        assert_eq!(elapsed(Duration::from_secs(600)), "10m ago");
        assert_eq!(elapsed(Duration::from_secs(7_300)), "2h ago");
        assert_eq!(elapsed(Duration::from_secs(200_000)), "2d ago");
    }

    #[test]
    fn outcomes_map_to_a_tone() {
        assert_eq!(tone(None), Tone::Quiet);
        assert_eq!(tone(Some(&KraddieOutcome::Working)), Tone::Live);
        assert_eq!(tone(Some(&KraddieOutcome::Finished)), Tone::Good);
        assert_eq!(tone(Some(&KraddieOutcome::NeedsYou)), Tone::Attention);
        assert_eq!(tone(Some(&KraddieOutcome::Interrupted)), Tone::Bad);
    }
}
