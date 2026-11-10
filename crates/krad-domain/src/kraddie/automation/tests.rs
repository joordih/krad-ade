use super::super::schedule::tests::{local, utc};
use super::super::schedule::{IntervalMinutes, Weekdays};
use super::*;
use crate::attention::TimeOfDay;
use crate::code_host::CodeHost;
use chrono::{Datelike, Utc, Weekday};

fn automation(triggers: Vec<Trigger>) -> Automation {
    let mut automation = Automation::new(
        AutomationId::FIRST,
        "Routine".parse().unwrap_or_else(|_| unreachable!()),
    );
    automation.triggers = triggers;
    automation
}

fn at(hour: u8, minute: u8, days: Weekdays) -> Trigger {
    Trigger::Schedule {
        schedule: KraddieSchedule::At {
            time: TimeOfDay::new(hour, minute).unwrap_or_else(|_| unreachable!()),
            days,
        },
    }
}

fn every(minutes: u32) -> Trigger {
    Trigger::Schedule {
        schedule: KraddieSchedule::Every {
            minutes: IntervalMinutes::try_from(minutes).unwrap_or_else(|_| unreachable!()),
        },
    }
}

fn utc_at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, day, hour, minute, 0)
        .single()
        .unwrap_or_else(|| unreachable!())
}

#[test]
fn the_next_run_is_the_earliest_of_the_automations_schedules() {
    let routine = automation(vec![
        Trigger::Manual,
        at(9, 0, Weekdays::WORKDAYS),
        at(
            7,
            30,
            Weekdays::new(&[Weekday::Sat]).unwrap_or_else(|_| unreachable!()),
        ),
    ]);
    let friday_noon = utc_at(13, 12, 0);
    let next = routine
        .next_after(&friday_noon)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(next, utc_at(14, 7, 30));
    assert_eq!(next.weekday(), Weekday::Sat);
    assert_eq!(routine.next_after(&next), Some(utc_at(16, 9, 0)));
    assert_eq!(
        automation(vec![Trigger::Manual]).next_after(&friday_noon),
        None
    );
}

#[test]
fn a_switched_off_automation_is_never_due_and_hears_nothing() {
    let mut routine = automation(vec![
        every(30),
        Trigger::App {
            event: AppTrigger::GitChanged,
        },
    ]);
    let since = utc_at(9, 10, 0);
    assert!(routine.is_due(&since, &utc_at(9, 10, 30)));
    assert!(!routine.is_due(&since, &utc_at(9, 10, 29)));
    assert!(routine.listens_for(AutomationEvent::App(AppTrigger::GitChanged)));
    assert!(!routine.listens_for(AutomationEvent::App(AppTrigger::AgentFinished)));
    routine.switch = Switch::Off;
    assert!(!routine.is_due(&since, &utc_at(9, 11, 0)));
    assert!(!routine.listens_for(AutomationEvent::App(AppTrigger::GitChanged)));
}

#[test]
fn code_host_events_reach_only_automations_of_that_host() {
    let routine = automation(vec![Trigger::CodeHost {
        host: CodeHost::Gitlab,
        event: CodeHostTrigger::CiFailed,
    }]);
    let on = |host| {
        AutomationEvent::CodeHost(CodeHostWake {
            host,
            event: CodeHostTrigger::CiFailed,
        })
    };
    assert!(routine.listens_for(on(CodeHost::Gitlab)));
    assert!(!routine.listens_for(on(CodeHost::Github)));
}

#[test]
fn a_daily_run_across_spring_forward_stays_on_its_local_time() {
    let routine = automation(vec![at(9, 0, Weekdays::EVERY_DAY)]);
    let next = routine
        .next_after(&local("2026-03-28 10:00"))
        .unwrap_or_else(|| unreachable!());
    assert_eq!(next.naive_local(), utc("2026-03-29 09:00"));
    assert_eq!(next.naive_utc(), utc("2026-03-29 07:00"));
}

#[test]
fn a_missed_run_inside_the_catch_up_window_runs_once_on_start() {
    let mut routine = automation(vec![at(9, 0, Weekdays::EVERY_DAY)]);
    let opened = utc_at(10, 9, 40);
    let ran_yesterday = utc_at(9, 9, 0);
    assert!(!routine.missed_run(Some(&ran_yesterday), &opened));
    routine.catch_up = CatchUp::Hour;
    assert!(routine.missed_run(Some(&ran_yesterday), &opened));
    assert!(routine.missed_run(None, &opened));
    let ran_today = utc_at(10, 9, 0);
    assert!(!routine.missed_run(Some(&ran_today), &opened));
    let too_late = utc_at(10, 10, 30);
    assert!(!routine.missed_run(Some(&ran_yesterday), &too_late));
    routine.catch_up = CatchUp::SixHours;
    assert!(routine.missed_run(Some(&ran_yesterday), &too_late));
    routine.switch = Switch::Off;
    assert!(!routine.missed_run(Some(&ran_yesterday), &too_late));
}

#[test]
fn a_missed_run_across_fall_back_is_found_in_local_time() {
    let mut routine = automation(vec![at(2, 30, Weekdays::EVERY_DAY)]);
    routine.catch_up = CatchUp::QuarterHour;
    let ran = local("2026-10-24 02:30");
    assert!(routine.missed_run(Some(&ran), &local("2026-10-25 02:40")));
    assert!(!routine.missed_run(Some(&ran), &local("2026-10-25 03:10")));
}

#[test]
fn validation_refuses_repeated_and_too_many_triggers() {
    assert_eq!(
        automation(vec![Trigger::Manual, Trigger::Manual]).validate(),
        Err(AutomationError::RepeatedTrigger)
    );
    let many = (1..=u32::try_from(MAX_TRIGGERS).unwrap_or_default() + 1)
        .map(every)
        .collect();
    assert_eq!(
        automation(many).validate(),
        Err(AutomationError::TooManyTriggers)
    );
    assert_eq!(automation(vec![Trigger::Manual]).validate(), Ok(()));
}

#[test]
fn an_exported_automation_forgets_the_users_group() {
    let mut routine = automation(vec![Trigger::Manual]);
    routine.session.working_copy = WorkingCopy::ThisRepo;
    routine.session.group = Some(std::path::PathBuf::from("/work/api"));
    let exported = routine.exported();
    assert_eq!(exported.session.group, None);
    assert_eq!(exported.session.working_copy, WorkingCopy::ThisRepo);
}

#[test]
fn effort_and_permission_overrides_keep_their_shape_on_disk() {
    let mut routine = automation(vec![Trigger::Manual]);
    routine.effort = Some(Effort::ExtraHigh);
    routine.permission_mode = Some(PermissionMode::AcceptEdits);
    let line = serde_json::to_string(&routine).unwrap_or_else(|_| unreachable!());
    assert!(line.contains(r#""effort":"xhigh""#), "{line}");
    assert!(
        line.contains(r#""permission_mode":"acceptEdits""#),
        "{line}"
    );
    assert_eq!(
        serde_json::from_str::<Automation>(&line).unwrap_or_else(|_| unreachable!()),
        routine
    );
}
