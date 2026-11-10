use super::activity::{KraddieActivity, KraddieDay};
use chrono::{Datelike, Days, Months, NaiveDate};

pub const HEATMAP_MONTHS: u32 = 6;
pub const DAYS_PER_WEEK: usize = 7;
pub const HEAT_LEVELS: usize = 5;
const MIN_LABEL_GAP_WEEKS: usize = 3;
const WEEK: Days = Days::new(DAYS_PER_WEEK as u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HeatMetric {
    #[default]
    Runs,
    LinesChanged,
    Tokens,
    Cost,
}

impl HeatMetric {
    pub const ALL: [Self; 4] = [Self::Runs, Self::LinesChanged, Self::Tokens, Self::Cost];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Runs => "Runs",
            Self::LinesChanged => "Lines changed",
            Self::Tokens => "Tokens",
            Self::Cost => "Cost",
        }
    }

    #[must_use]
    pub const fn value(self, day: &KraddieDay) -> u64 {
        match self {
            Self::Runs => day.runs as u64,
            Self::LinesChanged => day.lines_changed(),
            Self::Tokens => day.tokens(),
            Self::Cost => day.cost().nanos(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeatLevel(u8);

impl HeatLevel {
    pub const NONE: Self = Self(0);
    pub const ALL: [Self; HEAT_LEVELS] = [Self(0), Self(1), Self(2), Self(3), Self(4)];
    const TOP: u64 = 4;

    #[must_use]
    pub fn of(value: u64, max: u64) -> Self {
        if value == 0 || max == 0 {
            return Self::NONE;
        }
        let scaled = (u128::from(value) * u128::from(Self::TOP))
            .div_ceil(u128::from(max))
            .clamp(1, u128::from(Self::TOP));
        Self(u8::try_from(scaled).unwrap_or(u8::MAX))
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeatCell {
    pub date: NaiveDate,
    pub day: KraddieDay,
    pub value: u64,
    pub level: HeatLevel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthLabel {
    pub week: usize,
    pub month: chrono::Month,
}

impl MonthLabel {
    #[must_use]
    pub fn short_name(&self) -> &'static str {
        self.month.name().get(..3).unwrap_or_default()
    }
}

pub type HeatWeek = [Option<HeatCell>; DAYS_PER_WEEK];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeatMap {
    pub metric: HeatMetric,
    pub first: NaiveDate,
    pub last: NaiveDate,
    pub weeks: Vec<HeatWeek>,
    pub months: Vec<MonthLabel>,
    pub total: u64,
    pub max: u64,
}

impl HeatMap {
    #[must_use]
    pub fn build(activity: &KraddieActivity, today: NaiveDate, metric: HeatMetric) -> Self {
        let first = today
            .checked_sub_months(Months::new(HEATMAP_MONTHS))
            .and_then(|date| date.checked_add_days(Days::new(1)))
            .unwrap_or(today);
        let in_range = |date: NaiveDate| first <= date && date <= today;
        let max = activity
            .days()
            .filter(|(date, _)| in_range(*date))
            .map(|(_, day)| metric.value(day))
            .max()
            .unwrap_or_default();
        let back = u64::from(first.weekday().num_days_from_sunday());
        let start = first.checked_sub_days(Days::new(back)).unwrap_or(first);
        let mut weeks = Vec::new();
        let mut total = 0_u64;
        let mut week_start = start;
        while week_start <= today {
            let mut week: HeatWeek = [None; DAYS_PER_WEEK];
            for (offset, slot) in (0_u64..).zip(week.iter_mut()) {
                let Some(date) = week_start.checked_add_days(Days::new(offset)) else {
                    continue;
                };
                if !in_range(date) {
                    continue;
                }
                let day = activity.day(date);
                let value = metric.value(&day);
                total = total.saturating_add(value);
                *slot = Some(HeatCell {
                    date,
                    day,
                    value,
                    level: HeatLevel::of(value, max),
                });
            }
            weeks.push(week);
            let Some(next) = week_start.checked_add_days(WEEK) else {
                break;
            };
            week_start = next;
        }
        let months = month_labels(&weeks);
        Self {
            metric,
            first,
            last: today,
            weeks,
            months,
            total,
            max,
        }
    }
}

fn month_labels(weeks: &[HeatWeek]) -> Vec<MonthLabel> {
    let mut labels: Vec<MonthLabel> = Vec::new();
    let mut shown: Option<(i32, u32)> = None;
    for (index, week) in weeks.iter().enumerate() {
        let Some(cell) = week.iter().flatten().next() else {
            continue;
        };
        let month = (cell.date.year(), cell.date.month());
        if shown == Some(month) {
            continue;
        }
        shown = Some(month);
        let Some(name) = u8::try_from(cell.date.month())
            .ok()
            .and_then(|number| chrono::Month::try_from(number).ok())
        else {
            continue;
        };
        labels.push(MonthLabel {
            week: index,
            month: name,
        });
    }
    if let [first, second, ..] = labels.as_slice() {
        if second.week - first.week < MIN_LABEL_GAP_WEEKS {
            labels.remove(0);
        }
    }
    labels
}

#[cfg(test)]
mod tests;
