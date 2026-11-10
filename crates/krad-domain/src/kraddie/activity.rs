use crate::usage::{Spend, UsdCost};
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

pub const ACTIVITY_KEPT_DAYS: u64 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KraddieDate(pub NaiveDate);

impl fmt::Display for KraddieDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for KraddieDate {
    type Err = chrono::ParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse().map(Self)
    }
}

impl Serialize for KraddieDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KraddieDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KraddieDay {
    pub runs: u32,
    pub routines: u32,
    pub sessions: u32,
    pub prs_merged: u32,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub spend: Spend,
}

impl KraddieDay {
    #[must_use]
    pub const fn lines_changed(&self) -> u64 {
        self.lines_added.saturating_add(self.lines_removed)
    }

    #[must_use]
    pub const fn tokens(&self) -> u64 {
        self.spend.tokens.total()
    }

    #[must_use]
    pub const fn cost(&self) -> UsdCost {
        self.spend.cost
    }

    pub fn add(&mut self, other: &Self) {
        self.runs = self.runs.saturating_add(other.runs);
        self.routines = self.routines.saturating_add(other.routines);
        self.sessions = self.sessions.saturating_add(other.sessions);
        self.prs_merged = self.prs_merged.saturating_add(other.prs_merged);
        self.lines_added = self.lines_added.saturating_add(other.lines_added);
        self.lines_removed = self.lines_removed.saturating_add(other.lines_removed);
        self.spend += other.spend;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KraddieActivity {
    days: BTreeMap<KraddieDate, KraddieDay>,
}

impl KraddieActivity {
    pub fn record(&mut self, date: NaiveDate, delta: &KraddieDay) {
        self.days.entry(KraddieDate(date)).or_default().add(delta);
        self.prune(date);
    }

    pub fn prune(&mut self, today: NaiveDate) {
        let Some(oldest) = today.checked_sub_days(Days::new(ACTIVITY_KEPT_DAYS)) else {
            return;
        };
        self.days.retain(|date, _| date.0 > oldest);
    }

    #[must_use]
    pub fn day(&self, date: NaiveDate) -> KraddieDay {
        self.days
            .get(&KraddieDate(date))
            .copied()
            .unwrap_or_default()
    }

    pub fn days(&self) -> impl Iterator<Item = (NaiveDate, &KraddieDay)> {
        self.days.iter().map(|(date, day)| (date.0, day))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.days.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::TokenUsage;

    fn date(text: &str) -> NaiveDate {
        text.parse().unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn deltas_of_one_day_add_up() {
        let mut activity = KraddieActivity::default();
        let run = KraddieDay {
            runs: 1,
            sessions: 1,
            ..KraddieDay::default()
        };
        let ended = KraddieDay {
            lines_added: 12,
            lines_removed: 3,
            spend: Spend {
                tokens: TokenUsage {
                    input: 100,
                    output: 50,
                    ..TokenUsage::default()
                },
                cost: UsdCost::from_nanos(7),
            },
            ..KraddieDay::default()
        };
        activity.record(date("2026-11-09"), &run);
        activity.record(date("2026-11-09"), &ended);
        let day = activity.day(date("2026-11-09"));
        assert_eq!((day.runs, day.sessions, day.lines_changed()), (1, 1, 15));
        assert_eq!((day.tokens(), day.cost()), (150, UsdCost::from_nanos(7)));
        assert_eq!(activity.day(date("2026-11-08")), KraddieDay::default());
    }

    #[test]
    fn activity_keeps_a_bounded_window_of_days() {
        let mut activity = KraddieActivity::default();
        let today = date("2026-11-09");
        let one = KraddieDay {
            runs: 1,
            ..KraddieDay::default()
        };
        for back in 0..ACTIVITY_KEPT_DAYS + 30 {
            let day = today
                .checked_sub_days(Days::new(back))
                .unwrap_or_else(|| unreachable!());
            activity.days.insert(KraddieDate(day), one);
        }
        activity.record(today, &one);
        assert_eq!(
            activity.len(),
            usize::try_from(ACTIVITY_KEPT_DAYS).unwrap_or_else(|_| unreachable!())
        );
        assert_eq!(activity.day(today).runs, 2);
        let six_months_ago = date("2026-05-09");
        assert_eq!(activity.day(six_months_ago).runs, 1);
    }

    #[test]
    fn activity_files_read_back_and_tolerate_missing_fields() -> serde_json::Result<()> {
        let mut activity = KraddieActivity::default();
        activity.record(
            date("2026-10-01"),
            &KraddieDay {
                runs: 2,
                ..KraddieDay::default()
            },
        );
        let text = serde_json::to_string(&activity)?;
        assert!(text.contains("\"2026-10-01\""), "{text}");
        assert_eq!(serde_json::from_str::<KraddieActivity>(&text)?, activity);
        let old: KraddieActivity = serde_json::from_str(r#"{"days":{"2026-10-02":{"runs":1}}}"#)?;
        assert_eq!(old.day(date("2026-10-02")).runs, 1);
        assert_eq!(
            serde_json::from_str::<KraddieActivity>("{}")?,
            KraddieActivity::default()
        );
        assert!(serde_json::from_str::<KraddieActivity>(r#"{"days":{"nope":{}}}"#).is_err());
        Ok(())
    }
}
