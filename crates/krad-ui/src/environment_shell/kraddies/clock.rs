use chrono::{DateTime, Local};
use gpui_kit::Context;
use krad_domain::{
    code_host_name, KraddieSchedule, KraddieWake, Trigger, Weekdays, ANIMATION_FRAME,
};
use std::time::{Duration, SystemTime};

use super::super::EnvironmentShell;
use super::{AutomationEntry, KraddieEntry, KraddieWakeOrder};

const SCHEDULE_CHECK: Duration = Duration::from_secs(15);
const NEXT_RUN_FORMAT: &str = "%a %H:%M";
const EVERY_LABEL: &str = "Every";
const MINUTES_LABEL: &str = "min";
const HOURS_LABEL: &str = "h";
const MINUTES_PER_HOUR: u32 = 60;
const EVERY_DAY_LABEL: &str = "every day";
const WORKDAYS_LABEL: &str = "weekdays";
const DAY_SEPARATOR: &str = " ";
const HOST_SEPARATOR: &str = ": ";
const MANUAL_LABEL: &str = "Manual or krad CLI";

fn local(at: SystemTime) -> DateTime<Local> {
    DateTime::<Local>::from(at)
}

pub(super) fn since(entry: &AutomationEntry, opened_at: SystemTime) -> DateTime<Local> {
    let last = entry.history.last_started().unwrap_or(opened_at);
    local(last.max(opened_at))
}

pub(in crate::environment_shell) fn next_run(
    kraddie: &KraddieEntry,
    entry: &AutomationEntry,
    opened_at: SystemTime,
) -> Option<DateTime<Local>> {
    if !kraddie.kraddie.switch.is_on() || !entry.automation.switch.is_on() {
        return None;
    }
    entry.automation.next_after(&since(entry, opened_at))
}

pub(in crate::environment_shell) fn next_run_label(at: &DateTime<Local>) -> String {
    at.format(NEXT_RUN_FORMAT).to_string()
}

pub(super) fn next_run_stamp(at: &DateTime<Local>) -> String {
    at.to_rfc3339()
}

pub(in crate::environment_shell) fn schedule_label(schedule: KraddieSchedule) -> String {
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

pub(in crate::environment_shell) fn trigger_label(trigger: Trigger) -> String {
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

impl EnvironmentShell {
    pub(super) fn start_kraddie_clock(&mut self, cx: &mut Context<Self>) {
        self.kraddies.ticker = Some(cx.spawn(async move |this, cx| {
            let mut waited = Duration::ZERO;
            loop {
                cx.background_executor().timer(ANIMATION_FRAME).await;
                waited += ANIMATION_FRAME;
                let check = waited >= SCHEDULE_CHECK;
                if check {
                    waited = Duration::ZERO;
                }
                let alive = this.update(cx, |shell, cx| {
                    if check {
                        shell.wake_due_kraddies(cx);
                    }
                    if shell.kraddies.animated.replace(false) {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    fn wake_due_kraddies(&mut self, cx: &mut Context<Self>) {
        self.check_in_quiet_kraddies(cx);
        let now = Local::now();
        let opened_at = self.kraddies.opened_at;
        let due: Vec<KraddieWakeOrder> = self
            .kraddies
            .entries
            .iter()
            .filter(|kraddie| kraddie.kraddie.switch.is_on())
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .map(move |entry| (kraddie, entry))
            })
            .filter(|(kraddie, entry)| {
                !self
                    .kraddies
                    .limiter
                    .is_running(&kraddie.key(entry.automation.id))
            })
            .filter(|(_, entry)| entry.automation.is_due(&since(entry, opened_at), &now))
            .map(|(kraddie, entry)| KraddieWakeOrder {
                key: kraddie.key(entry.automation.id),
                wake: KraddieWake::Schedule,
                prompt: None,
            })
            .collect();
        if due.is_empty() {
            return;
        }
        self.kraddies.wakes.extend(due);
        self.flush_kraddies(cx);
    }

    pub(super) fn queue_missed_runs(&mut self) {
        let opened = local(self.kraddies.opened_at);
        let missed: Vec<KraddieWakeOrder> = self
            .kraddies
            .entries
            .iter()
            .filter(|kraddie| kraddie.kraddie.switch.is_on())
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .map(move |entry| (kraddie, entry))
            })
            .filter(|(_, entry)| {
                let last = entry.history.last_started().map(local);
                entry.automation.missed_run(last.as_ref(), &opened)
            })
            .map(|(kraddie, entry)| KraddieWakeOrder {
                key: kraddie.key(entry.automation.id),
                wake: KraddieWake::CatchUp,
                prompt: None,
            })
            .collect();
        self.kraddies.wakes.extend(missed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AppTrigger, CodeHost, CodeHostTrigger, IntervalMinutes, TimeOfDay};

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
