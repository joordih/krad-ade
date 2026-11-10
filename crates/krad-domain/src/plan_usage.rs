mod reading;
mod reset;
mod schedule;

use crate::warn_percent::{lenient_warn_percent, WarnPercent};
use serde::{Deserialize, Deserializer, Serialize};
use std::time::{Duration, SystemTime};

pub use reading::{UsageFailure, UsageReading};
pub use reset::ResetTime;
pub use schedule::{
    UsageBudget, UsageCadence, UsageSchedule, UsageTrigger, USAGE_REFRESH_INTERVAL,
};

const FIVE_HOURS: Duration = Duration::from_hours(5);
const WEEK: Duration = Duration::from_hours(168);
const DEFAULT_PLAN_WARNING: WarnPercent = WarnPercent::known(80);
pub const UNTIMED_ALERT_LASTS: Duration = Duration::from_mins(30);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WindowSpan {
    FiveHours,
    Weekly,
    Monthly,
    Rolling,
    Lasting(Duration),
    Named(String),
}

impl WindowSpan {
    const fn rank(&self) -> (u8, Duration) {
        match self {
            Self::FiveHours => (0, Duration::ZERO),
            Self::Rolling => (1, Duration::ZERO),
            Self::Lasting(duration) => (2, *duration),
            Self::Weekly => (3, Duration::ZERO),
            Self::Monthly => (4, Duration::ZERO),
            Self::Named(_) => (5, Duration::ZERO),
        }
    }

    #[must_use]
    pub fn lasting(duration: Duration) -> Self {
        match duration {
            FIVE_HOURS => Self::FiveHours,
            WEEK => Self::Weekly,
            other => Self::Lasting(other),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageWindow {
    pub span: WindowSpan,
    pub scope: Option<String>,
    pub used: u16,
    pub resets_at: Option<ResetTime>,
    pub limited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitStatus {
    Warning,
    Reached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitAlert {
    pub status: LimitStatus,
    pub resets_at: Option<ResetTime>,
    pub heard_at: SystemTime,
}

impl LimitAlert {
    #[must_use]
    pub fn is_over(self, now: SystemTime) -> bool {
        match self.resets_at {
            Some(reset) => reset.left(now).is_zero(),
            None => now
                .duration_since(self.heard_at)
                .is_ok_and(|heard| heard >= UNTIMED_ALERT_LASTS),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanUsage {
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub alert: Option<LimitAlert>,
}

impl PlanUsage {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty() && self.alert.is_none()
    }

    #[must_use]
    pub fn live_alert(&self, now: SystemTime) -> Option<LimitAlert> {
        self.alert.filter(|alert| !alert.is_over(now))
    }

    #[must_use]
    pub fn ranked(&self) -> Vec<&UsageWindow> {
        let mut ranked: Vec<&UsageWindow> = self.windows.iter().collect();
        ranked.sort_by_key(|window| (window.scope.is_some(), window.span.rank()));
        ranked
    }

    #[must_use]
    pub fn primary(&self) -> Option<&UsageWindow> {
        self.ranked().into_iter().next()
    }

    #[must_use]
    pub fn window(&self, span: &WindowSpan) -> Option<&UsageWindow> {
        self.windows
            .iter()
            .find(|window| &window.span == span && window.scope.is_none())
    }

    pub fn place(&mut self, window: UsageWindow) {
        match self
            .windows
            .iter_mut()
            .find(|shown| shown.span == window.span && shown.scope == window.scope)
        {
            Some(shown) => *shown = window,
            None => self.windows.push(window),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UsageIndicator {
    #[default]
    Shown,
    Hidden,
}

const fn default_plan_warning() -> WarnPercent {
    DEFAULT_PLAN_WARNING
}

fn lenient_plan_warning<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<WarnPercent, D::Error> {
    lenient_warn_percent(deserializer, DEFAULT_PLAN_WARNING)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanUsageSettings {
    #[serde(default)]
    pub indicator: UsageIndicator,
    #[serde(
        default = "default_plan_warning",
        deserialize_with = "lenient_plan_warning"
    )]
    pub warn_at: WarnPercent,
}

impl Default for PlanUsageSettings {
    fn default() -> Self {
        Self {
            indicator: UsageIndicator::Shown,
            warn_at: DEFAULT_PLAN_WARNING,
        }
    }
}

impl PlanUsageSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub fn warns(&self, usage: &PlanUsage) -> bool {
        usage.alert.is_some()
            || usage
                .windows
                .iter()
                .any(|window| window.limited || self.warn_at.reached_by(window.used))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(span: WindowSpan, used: u16) -> UsageWindow {
        UsageWindow {
            span,
            scope: None,
            used,
            resets_at: None,
            limited: false,
        }
    }

    #[test]
    fn a_window_of_five_hours_or_a_week_reads_as_the_named_window() {
        assert_eq!(WindowSpan::lasting(FIVE_HOURS), WindowSpan::FiveHours);
        assert_eq!(WindowSpan::lasting(WEEK), WindowSpan::Weekly);
        assert_eq!(
            WindowSpan::lasting(Duration::from_hours(1)),
            WindowSpan::Lasting(Duration::from_hours(1))
        );
    }

    #[test]
    fn placing_a_window_replaces_the_one_of_the_same_span_and_scope() {
        let mut usage = PlanUsage::default();
        usage.place(window(WindowSpan::FiveHours, 10));
        usage.place(window(WindowSpan::Weekly, 40));
        usage.place(window(WindowSpan::FiveHours, 15));
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(
            usage.window(&WindowSpan::FiveHours).map(|w| w.used),
            Some(15)
        );
        assert_eq!(usage.primary().map(|w| w.used), Some(15));
    }

    #[test]
    fn the_primary_window_is_the_five_hour_or_rolling_one_and_falls_back_to_weekly() {
        let mut opus = window(WindowSpan::FiveHours, 90);
        opus.scope = Some("Opus".to_owned());
        let mut usage = PlanUsage {
            windows: vec![
                opus,
                window(WindowSpan::Monthly, 70),
                window(WindowSpan::Weekly, 40),
            ],
            ..PlanUsage::default()
        };
        assert_eq!(
            usage.primary().map(|w| w.span.clone()),
            Some(WindowSpan::Weekly)
        );
        usage.place(window(WindowSpan::Rolling, 5));
        assert_eq!(usage.primary().map(|w| w.used), Some(5));
        usage.place(window(WindowSpan::FiveHours, 12));
        let spans: Vec<(WindowSpan, bool)> = usage
            .ranked()
            .into_iter()
            .map(|w| (w.span.clone(), w.scope.is_some()))
            .collect();
        assert_eq!(
            spans,
            [
                (WindowSpan::FiveHours, false),
                (WindowSpan::Rolling, false),
                (WindowSpan::Weekly, false),
                (WindowSpan::Monthly, false),
                (WindowSpan::FiveHours, true),
            ]
        );
        assert_eq!(PlanUsage::default().primary(), None);
    }

    #[test]
    fn usage_warns_at_the_chosen_percent_on_a_hit_limit_or_an_alert() {
        let settings = PlanUsageSettings::default();
        let mut usage = PlanUsage {
            windows: vec![window(WindowSpan::FiveHours, 79)],
            ..PlanUsage::default()
        };
        assert!(!settings.warns(&usage));
        usage.windows[0].used = 80;
        assert!(settings.warns(&usage));
        usage.windows[0].used = 10;
        usage.windows[0].limited = true;
        assert!(settings.warns(&usage));
        let alerted = PlanUsage {
            alert: Some(LimitAlert {
                status: LimitStatus::Warning,
                resets_at: None,
                heard_at: SystemTime::UNIX_EPOCH,
            }),
            ..PlanUsage::default()
        };
        assert!(settings.warns(&alerted));
    }

    #[test]
    fn an_alert_ends_when_its_limit_resets_or_after_a_while_without_a_reset_time() {
        let heard_at = SystemTime::UNIX_EPOCH + Duration::from_hours(1);
        let timed = LimitAlert {
            status: LimitStatus::Reached,
            resets_at: Some(ResetTime::from_epoch_secs(2 * 3600)),
            heard_at,
        };
        assert!(!timed.is_over(heard_at));
        assert!(timed.is_over(heard_at + Duration::from_hours(1)));
        let untimed = LimitAlert {
            resets_at: None,
            ..timed
        };
        assert!(!untimed.is_over(heard_at + UNTIMED_ALERT_LASTS - Duration::from_secs(1)));
        assert!(untimed.is_over(heard_at + UNTIMED_ALERT_LASTS));
        let usage = PlanUsage {
            alert: Some(untimed),
            ..PlanUsage::default()
        };
        assert_eq!(usage.live_alert(heard_at), Some(untimed));
        assert_eq!(usage.live_alert(heard_at + UNTIMED_ALERT_LASTS), None);
    }

    #[test]
    fn a_hidden_usage_bar_and_its_warning_survive_a_save_and_load() {
        let hidden = PlanUsageSettings {
            indicator: UsageIndicator::Hidden,
            warn_at: WarnPercent::known(90),
        };
        let saved = serde_json::to_string(&hidden).ok();
        let loaded: Option<PlanUsageSettings> =
            saved.and_then(|saved| serde_json::from_str(&saved).ok());
        assert_eq!(loaded, Some(hidden));
        assert!(PlanUsageSettings::default().is_default());
        assert_eq!(
            PlanUsageSettings::default().indicator,
            UsageIndicator::Shown
        );
    }

    #[test]
    fn plan_usage_settings_saved_before_or_out_of_range_load_with_defaults() {
        let older: Option<PlanUsageSettings> = serde_json::from_str("{}").ok();
        assert_eq!(older, Some(PlanUsageSettings::default()));
        let odd: Option<PlanUsageSettings> =
            serde_json::from_str(r#"{"indicator":"Hidden","warn_at":0}"#).ok();
        assert_eq!(
            odd,
            Some(PlanUsageSettings {
                indicator: UsageIndicator::Hidden,
                warn_at: DEFAULT_PLAN_WARNING,
            })
        );
    }
}
