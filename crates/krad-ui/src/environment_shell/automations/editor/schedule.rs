use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{IntervalMinutes, KraddieSchedule, TimeOfDay, Weekdays};

use super::super::super::dialog_parts::{segment, SegmentSize};
use super::super::super::EnvironmentShell;
use super::cue::REMOVE_TOOLTIP;
use super::parts::{chips, panel};
use crate::components::{Button, IconName};
use crate::theme::{Palette, SPACE_1_5};

const SCHEDULE_REMOVE_PREFIX: &str = "automation-schedule-remove";
const KIND_PREFIX: &str = "automation-schedule-kind";
const DAY_PREFIX: &str = "automation-schedule-day";
const STEP: u64 = 1000;

const DEFAULT_INTERVAL: &str = "60";
const DEFAULT_TIME: &str = "09:00";
const INTERVAL_PLACEHOLDER: &str = "Minutes between runs";
const TIME_PLACEHOLDER: &str = "HH:MM";
pub(super) const BAD_INTERVAL: &str = "Use a whole number of minutes, from 1 up to 10080";
pub(super) const BAD_TIME: &str = "Write the hour as HH:MM, for example 08:30";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScheduleKind {
    Every,
    At,
}

impl ScheduleKind {
    pub(super) const ALL: [Self; 2] = [Self::Every, Self::At];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Every => "On a rhythm",
            Self::At => "At a set hour",
        }
    }
}

pub(in crate::environment_shell) struct ScheduleRow {
    pub(super) kind: ScheduleKind,
    pub(super) days: Weekdays,
    pub(super) interval: Entity<InputState>,
    pub(super) time: Entity<InputState>,
}

fn parts(schedule: KraddieSchedule) -> (ScheduleKind, String, String, Weekdays) {
    match schedule {
        KraddieSchedule::Every { minutes } => (
            ScheduleKind::Every,
            minutes.get().to_string(),
            DEFAULT_TIME.to_owned(),
            Weekdays::WORKDAYS,
        ),
        KraddieSchedule::At { time, days } => (
            ScheduleKind::At,
            DEFAULT_INTERVAL.to_owned(),
            time.to_string(),
            days,
        ),
    }
}

pub(super) fn read_schedule(
    kind: ScheduleKind,
    interval: &str,
    time: &str,
    days: Weekdays,
) -> Result<KraddieSchedule, &'static str> {
    match kind {
        ScheduleKind::Every => interval
            .trim()
            .parse::<u32>()
            .ok()
            .and_then(|minutes| IntervalMinutes::try_from(minutes).ok())
            .map(|minutes| KraddieSchedule::Every { minutes })
            .ok_or(BAD_INTERVAL),
        ScheduleKind::At => time
            .parse::<TimeOfDay>()
            .map(|time| KraddieSchedule::At { time, days })
            .map_err(|_| BAD_TIME),
    }
}

impl ScheduleRow {
    pub(super) fn new(
        schedule: KraddieSchedule,
        window: &mut Window,
        cx: &mut Context<EnvironmentShell>,
    ) -> Self {
        let (kind, interval, time, days) = parts(schedule);
        Self {
            kind,
            days,
            interval: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(INTERVAL_PLACEHOLDER)
                    .default_value(interval)
            }),
            time: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(TIME_PLACEHOLDER)
                    .default_value(time)
            }),
        }
    }

    pub(super) fn read(
        &self,
        cx: &Context<EnvironmentShell>,
    ) -> Result<KraddieSchedule, &'static str> {
        read_schedule(
            self.kind,
            &self.interval.read(cx).value(),
            &self.time.read(cx).value(),
            self.days,
        )
    }
}

impl EnvironmentShell {
    pub(super) fn schedule_row(
        index: usize,
        row: &ScheduleRow,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let base = u64::try_from(index).unwrap_or_default() * STEP;
        let kinds = ScheduleKind::ALL.into_iter().enumerate().map(|(at, kind)| {
            segment(
                (KIND_PREFIX, base + u64::try_from(at).unwrap_or_default()),
                kind.label(),
                Some(IconName::Repeat),
                row.kind == kind,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_automation_draft(cx, |editor| {
                    if let Some(row) = editor.schedules.get_mut(index) {
                        row.kind = kind;
                    }
                });
            }))
            .into_any_element()
        });
        let kinds: Vec<AnyElement> = kinds.collect();
        let days: Vec<AnyElement> = Weekdays::all()
            .into_iter()
            .enumerate()
            .map(|(at, day)| {
                segment(
                    (DAY_PREFIX, base + u64::try_from(at).unwrap_or_default()),
                    Weekdays::short_name(day),
                    None,
                    row.days.contains(day),
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_automation_draft(cx, |editor| {
                        if let Some(row) = editor.schedules.get_mut(index) {
                            row.days = row.days.toggled(day).unwrap_or(row.days);
                        }
                    });
                }))
                .into_any_element()
            })
            .collect();
        let field = match row.kind {
            ScheduleKind::Every => Input::new(&row.interval).into_any_element(),
            ScheduleKind::At => Input::new(&row.time).into_any_element(),
        };
        panel(pal)
            .child(
                chips().children(kinds).child(div().flex_1()).child(
                    Button::new((SCHEDULE_REMOVE_PREFIX, index))
                        .icon(IconName::Close)
                        .small()
                        .tooltip(REMOVE_TOOLTIP)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.edit_automation_draft(cx, |editor| {
                                if index < editor.schedules.len() {
                                    editor.schedules.remove(index);
                                }
                            });
                        })),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(SPACE_1_5))
                    .child(div().w_full().child(field)),
            )
            .when(row.kind == ScheduleKind::At, |column| {
                column.child(chips().children(days))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_schedule_row_reads_back_into_the_schedule_it_shows() {
        let days = Weekdays::WORKDAYS;
        let every = read_schedule(ScheduleKind::Every, " 30 ", "", days)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(parts(every).1, "30");
        assert_eq!(
            read_schedule(ScheduleKind::Every, "0", "", days),
            Err(BAD_INTERVAL)
        );
        assert_eq!(
            read_schedule(ScheduleKind::Every, "soon", "", days),
            Err(BAD_INTERVAL)
        );
        assert_eq!(
            read_schedule(ScheduleKind::At, "", "25:00", days),
            Err(BAD_TIME)
        );
        let at = read_schedule(ScheduleKind::At, "", "07:15", days)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            parts(at),
            (
                ScheduleKind::At,
                DEFAULT_INTERVAL.to_owned(),
                "07:15".to_owned(),
                days
            )
        );
    }
}
