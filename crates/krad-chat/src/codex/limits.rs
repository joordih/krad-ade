use krad_domain::{PlanUsage, ResetTime, UsageWindow, WindowSpan};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub(crate) const RATE_LIMITS_READ: &str = "account/rateLimits/read";
const FULL_PERCENT: f64 = 100.0;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadParams {
    exclude_reset_credit_details: bool,
}

impl Default for ReadParams {
    fn default() -> Self {
        Self {
            exclude_reset_credit_details: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LimitWindow {
    used_percent: Option<f64>,
    window_duration_mins: Option<u32>,
    resets_at: Option<u64>,
}

impl LimitWindow {
    fn merge(&mut self, update: Self) {
        self.used_percent = update.used_percent.or(self.used_percent);
        self.window_duration_mins = update.window_duration_mins.or(self.window_duration_mins);
        self.resets_at = update.resets_at.or(self.resets_at);
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn shown(&self, slot: Slot, scope: Option<&str>) -> Option<UsageWindow> {
        let used = self.used_percent?;
        if !used.is_finite() {
            return None;
        }
        let span = self.window_duration_mins.map_or_else(
            || slot.span(),
            |mins| WindowSpan::lasting(Duration::from_mins(u64::from(mins))),
        );
        Some(UsageWindow {
            span,
            scope: scope.map(str::to_owned),
            used: used.round().clamp(0.0, FULL_PERCENT) as u16,
            resets_at: self.resets_at.map(ResetTime::from_epoch),
            limited: false,
        })
    }
}

#[derive(Clone, Copy)]
enum Slot {
    Primary,
    Secondary,
}

impl Slot {
    const fn span(self) -> WindowSpan {
        match self {
            Self::Primary => WindowSpan::FiveHours,
            Self::Secondary => WindowSpan::Weekly,
        }
    }
}

fn merge_window(kept: &mut Option<LimitWindow>, update: Option<LimitWindow>) {
    let Some(update) = update else {
        return;
    };
    match kept {
        Some(kept) => kept.merge(update),
        None => *kept = Some(update),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LimitSnapshot {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    primary: Option<LimitWindow>,
    secondary: Option<LimitWindow>,
}

impl LimitSnapshot {
    fn merge(&mut self, update: Self) {
        self.limit_name = update.limit_name.or(self.limit_name.take());
        self.plan_type = update.plan_type.or(self.plan_type.take());
        merge_window(&mut self.primary, update.primary);
        merge_window(&mut self.secondary, update.secondary);
    }

    fn windows(&self, scope: Option<&str>) -> impl Iterator<Item = UsageWindow> + '_ {
        let scope = scope.map(str::to_owned);
        [
            (Slot::Primary, self.primary.as_ref()),
            (Slot::Secondary, self.secondary.as_ref()),
        ]
        .into_iter()
        .filter_map(move |(slot, window)| window?.shown(slot, scope.as_deref()))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LimitsRead {
    rate_limits: Option<LimitSnapshot>,
    #[serde(default)]
    rate_limits_by_limit_id: HashMap<String, LimitSnapshot>,
}

#[derive(Debug, Default)]
pub(crate) struct LimitBook {
    main: Option<String>,
    limits: Vec<(String, LimitSnapshot)>,
}

impl LimitBook {
    pub(crate) fn read(&mut self, read: LimitsRead) -> PlanUsage {
        if let Some(main) = read.rate_limits {
            self.main = main.limit_id.clone().or(self.main.take());
            self.merge(main);
        }
        let mut others: Vec<_> = read.rate_limits_by_limit_id.into_iter().collect();
        others.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (id, mut snapshot) in others {
            snapshot.limit_id.get_or_insert(id);
            self.merge(snapshot);
        }
        self.usage()
    }

    pub(crate) fn update(&mut self, snapshot: LimitSnapshot) -> PlanUsage {
        self.merge(snapshot);
        self.usage()
    }

    fn merge(&mut self, snapshot: LimitSnapshot) {
        let id = snapshot
            .limit_id
            .clone()
            .or_else(|| self.main.clone())
            .unwrap_or_default();
        if self.main.is_none() {
            self.main = Some(id.clone());
        }
        match self.limits.iter_mut().find(|(kept, _)| *kept == id) {
            Some((_, kept)) => kept.merge(snapshot),
            None => self.limits.push((id, snapshot)),
        }
    }

    fn usage(&self) -> PlanUsage {
        let is_main = |id: &str| self.main.as_deref() == Some(id);
        let main = self.limits.iter().find(|(id, _)| is_main(id));
        let others = self.limits.iter().filter(|(id, _)| !is_main(id));
        let mut usage = PlanUsage {
            plan: main.and_then(|(_, snapshot)| snapshot.plan_type.clone()),
            ..PlanUsage::default()
        };
        for window in main
            .into_iter()
            .flat_map(|(_, snapshot)| snapshot.windows(None))
        {
            usage.place(window);
        }
        for (id, snapshot) in others {
            let scope = snapshot.limit_name.as_deref().unwrap_or(id);
            for window in snapshot.windows(Some(scope)) {
                usage.place(window);
            }
        }
        usage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(value: serde_json::Value) -> LimitsRead {
        serde_json::from_value(value).unwrap_or_default()
    }

    fn snapshot(value: serde_json::Value) -> LimitSnapshot {
        serde_json::from_value(value).unwrap_or_default()
    }

    fn reading() -> serde_json::Value {
        json!({
            "rateLimits": {"limitId": "codex", "limitName": null, "planType": "pro",
                "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1_794_249_000},
                "secondary": {"usedPercent": 40.4, "windowDurationMins": 10_080, "resetsAt": 1_794_800_000},
                "credits": {"hasCredits": false, "unlimited": false, "balance": null}},
            "rateLimitsByLimitId": {
                "codex": {"limitId": "codex", "planType": "pro",
                    "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1_794_249_000}},
                "codex_other": {"limitId": "codex_other", "limitName": "GPT-5.5-Codex-Spark",
                    "primary": {"usedPercent": 3, "windowDurationMins": 300, "resetsAt": null}}
            }
        })
    }

    #[test]
    fn a_reading_shows_each_window_its_reset_and_the_plan() {
        let usage = LimitBook::default().read(read(reading()));
        assert_eq!(usage.plan.as_deref(), Some("pro"));
        assert_eq!(
            usage.window(&WindowSpan::FiveHours),
            Some(&UsageWindow {
                span: WindowSpan::FiveHours,
                scope: None,
                used: 12,
                resets_at: Some(ResetTime::from_epoch_secs(1_794_249_000)),
                limited: false,
            })
        );
        assert_eq!(usage.window(&WindowSpan::Weekly).map(|w| w.used), Some(40));
        let spark = usage
            .windows
            .iter()
            .find(|window| window.scope.as_deref() == Some("GPT-5.5-Codex-Spark"));
        assert_eq!(
            spark.map(|window| (window.used, window.resets_at)),
            Some((3, None))
        );
    }

    #[test]
    fn a_sparse_update_changes_only_what_it_carries() {
        let mut book = LimitBook::default();
        book.read(read(reading()));
        let usage = book.update(snapshot(json!({"limitId": "codex",
            "primary": {"usedPercent": 30}})));
        let five = usage.window(&WindowSpan::FiveHours);
        assert_eq!(five.map(|window| window.used), Some(30));
        assert_eq!(
            five.and_then(|window| window.resets_at),
            Some(ResetTime::from_epoch_secs(1_794_249_000))
        );
        assert_eq!(usage.window(&WindowSpan::Weekly).map(|w| w.used), Some(40));
        assert_eq!(usage.plan.as_deref(), Some("pro"));
    }

    #[test]
    fn an_update_without_a_limit_id_belongs_to_the_main_limit() {
        let mut book = LimitBook::default();
        book.read(read(reading()));
        let usage = book.update(snapshot(json!({"secondary": {"usedPercent": 55}})));
        assert_eq!(usage.window(&WindowSpan::Weekly).map(|w| w.used), Some(55));
    }

    #[test]
    fn a_window_without_its_length_takes_the_span_of_its_position() {
        let usage = LimitBook::default().read(read(json!({"rateLimits": {"limitId": "codex",
            "primary": {"usedPercent": 12, "resetsAt": 1_794_249_000_000_u64},
            "secondary": {"usedPercent": 30}}})));
        assert_eq!(
            usage
                .window(&WindowSpan::FiveHours)
                .map(|w| (w.used, w.resets_at)),
            Some((12, Some(ResetTime::from_epoch_secs(1_794_249_000))))
        );
        assert_eq!(usage.window(&WindowSpan::Weekly).map(|w| w.used), Some(30));
    }

    #[test]
    fn a_window_without_its_share_is_left_out() {
        let usage = LimitBook::default().read(read(json!({"rateLimits": {"limitId": "codex",
            "primary": {"windowDurationMins": 300}, "secondary": null}})));
        assert_eq!(usage.windows, Vec::<UsageWindow>::new());
        let empty = LimitBook::default().read(read(json!({})));
        assert!(empty.is_empty());
    }
}
