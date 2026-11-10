use crate::attention::Switch;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

const MINUTE: u64 = 60;
pub const QUIET_FIFTEEN_MINUTES: Duration = Duration::from_secs(15 * MINUTE);
pub const QUIET_THIRTY_MINUTES: Duration = Duration::from_secs(30 * MINUTE);
pub const QUIET_ONE_HOUR: Duration = Duration::from_secs(60 * MINUTE);
pub const QUIET_TWO_HOURS: Duration = Duration::from_secs(120 * MINUTE);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuietSpan {
    FifteenMinutes,
    #[default]
    ThirtyMinutes,
    OneHour,
    TwoHours,
}

impl QuietSpan {
    pub const ALL: [Self; 4] = [
        Self::FifteenMinutes,
        Self::ThirtyMinutes,
        Self::OneHour,
        Self::TwoHours,
    ];

    #[must_use]
    pub const fn duration(self) -> Duration {
        match self {
            Self::FifteenMinutes => QUIET_FIFTEEN_MINUTES,
            Self::ThirtyMinutes => QUIET_THIRTY_MINUTES,
            Self::OneHour => QUIET_ONE_HOUR,
            Self::TwoHours => QUIET_TWO_HOURS,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::FifteenMinutes => "15m",
            Self::ThirtyMinutes => "30m",
            Self::OneHour => "1h",
            Self::TwoHours => "2h",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KraddieRounds {
    pub switch: Switch,
    pub after: QuietSpan,
}

impl Default for KraddieRounds {
    fn default() -> Self {
        Self {
            switch: Switch::Off,
            after: QuietSpan::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunSlot {
    #[default]
    Free,
    KraddieRunning,
    AllBusy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hush {
    #[default]
    None,
    QuietHours,
    OverBudget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundsMoment {
    pub quiet_since: SystemTime,
    pub now: SystemTime,
    pub kraddie: Switch,
    pub slot: RunSlot,
    pub hush: Hush,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundsCheck {
    Due,
    Off,
    NotQuietYet,
    Running,
    AtCapacity,
    QuietHours,
    OverBudget,
}

impl KraddieRounds {
    #[must_use]
    pub fn check(&self, moment: &RoundsMoment) -> RoundsCheck {
        if !self.switch.is_on() || !moment.kraddie.is_on() {
            return RoundsCheck::Off;
        }
        if moment.slot == RunSlot::KraddieRunning {
            return RoundsCheck::Running;
        }
        let quiet = moment
            .now
            .duration_since(moment.quiet_since)
            .unwrap_or_default();
        if quiet < self.after.duration() {
            return RoundsCheck::NotQuietYet;
        }
        match (moment.hush, moment.slot) {
            (Hush::QuietHours, _) => RoundsCheck::QuietHours,
            (Hush::OverBudget, _) => RoundsCheck::OverBudget,
            (Hush::None, RunSlot::AllBusy) => RoundsCheck::AtCapacity,
            (Hush::None, RunSlot::Free | RunSlot::KraddieRunning) => RoundsCheck::Due,
        }
    }

    #[must_use]
    pub fn next_after(&self, quiet_since: SystemTime) -> Option<SystemTime> {
        self.switch
            .is_on()
            .then(|| quiet_since + self.after.duration())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment(quiet_for: Duration) -> RoundsMoment {
        let since = SystemTime::UNIX_EPOCH;
        RoundsMoment {
            quiet_since: since,
            now: since + quiet_for,
            kraddie: Switch::On,
            slot: RunSlot::Free,
            hush: Hush::None,
        }
    }

    fn on(after: QuietSpan) -> KraddieRounds {
        KraddieRounds {
            switch: Switch::On,
            after,
        }
    }

    #[test]
    fn the_rounds_is_off_until_chosen() {
        let beat = KraddieRounds::default();
        assert_eq!(beat.check(&moment(QUIET_TWO_HOURS)), RoundsCheck::Off);
        assert_eq!(beat.next_after(SystemTime::UNIX_EPOCH), None);
        let mut switched_off_kraddie = moment(QUIET_TWO_HOURS);
        switched_off_kraddie.kraddie = Switch::Off;
        assert_eq!(
            on(QuietSpan::FifteenMinutes).check(&switched_off_kraddie),
            RoundsCheck::Off
        );
    }

    #[test]
    fn a_check_in_waits_for_the_chosen_quiet_time() {
        let beat = on(QuietSpan::OneHour);
        assert_eq!(
            beat.check(&moment(QUIET_THIRTY_MINUTES)),
            RoundsCheck::NotQuietYet
        );
        assert_eq!(beat.check(&moment(QUIET_ONE_HOUR)), RoundsCheck::Due);
        let mut back_in_time = moment(Duration::ZERO);
        back_in_time.now = SystemTime::UNIX_EPOCH;
        back_in_time.quiet_since = SystemTime::UNIX_EPOCH + QUIET_TWO_HOURS;
        assert_eq!(beat.check(&back_in_time), RoundsCheck::NotQuietYet);
        assert_eq!(
            beat.next_after(SystemTime::UNIX_EPOCH),
            Some(SystemTime::UNIX_EPOCH + QUIET_ONE_HOUR)
        );
        let spans: Vec<&str> = QuietSpan::ALL.iter().map(|span| span.label()).collect();
        assert_eq!(spans, vec!["15m", "30m", "1h", "2h"]);
    }

    #[test]
    fn a_check_in_respects_runs_limits_quiet_hours_and_the_budget() {
        let beat = on(QuietSpan::FifteenMinutes);
        let quiet = moment(QUIET_ONE_HOUR);
        let running = RoundsMoment {
            slot: RunSlot::KraddieRunning,
            ..quiet
        };
        assert_eq!(beat.check(&running), RoundsCheck::Running);
        let full = RoundsMoment {
            slot: RunSlot::AllBusy,
            ..quiet
        };
        assert_eq!(beat.check(&full), RoundsCheck::AtCapacity);
        let night = RoundsMoment {
            hush: Hush::QuietHours,
            ..quiet
        };
        assert_eq!(beat.check(&night), RoundsCheck::QuietHours);
        let spent = RoundsMoment {
            hush: Hush::OverBudget,
            ..quiet
        };
        assert_eq!(beat.check(&spent), RoundsCheck::OverBudget);
    }
}
