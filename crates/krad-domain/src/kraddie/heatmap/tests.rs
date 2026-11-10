use super::*;
use crate::usage::{Spend, TokenUsage, UsdCost};
use chrono::Weekday;

fn date(text: &str) -> NaiveDate {
    text.parse().unwrap_or_else(|_| unreachable!())
}

fn runs(count: u32) -> KraddieDay {
    KraddieDay {
        runs: count,
        ..KraddieDay::default()
    }
}

#[test]
fn the_grid_covers_six_months_in_weeks_starting_on_sunday() {
    let today = date("2026-11-09");
    let map = HeatMap::build(&KraddieActivity::default(), today, HeatMetric::Runs);
    assert_eq!(map.first, date("2026-05-10"));
    assert_eq!(map.last, today);
    assert_eq!(map.weeks.len(), 27);
    let first_week = map.weeks.first().unwrap_or_else(|| unreachable!());
    assert_eq!(
        first_week[0].map(|cell| cell.date),
        Some(date("2026-05-10"))
    );
    assert_eq!(date("2026-05-10").weekday(), Weekday::Sun);
    let last_week = map.weeks.last().unwrap_or_else(|| unreachable!());
    assert_eq!(last_week[1].map(|cell| cell.date), Some(today));
    assert!(last_week[2..].iter().all(Option::is_none));
    let cells = map.weeks.iter().flatten().flatten().count();
    assert_eq!(cells, 184);
    assert_eq!((map.total, map.max), (0, 0));
    assert!(map
        .weeks
        .iter()
        .flatten()
        .flatten()
        .all(|cell| cell.level == HeatLevel::NONE));
}

#[test]
fn days_before_the_range_are_blank_cells() {
    let today = date("2026-11-11");
    let map = HeatMap::build(&KraddieActivity::default(), today, HeatMetric::Runs);
    assert_eq!(map.first, date("2026-05-12"));
    let first_week = map.weeks.first().unwrap_or_else(|| unreachable!());
    assert!(first_week[..2].iter().all(Option::is_none));
    assert_eq!(first_week[2].map(|cell| cell.date), Some(map.first));
}

#[test]
fn intensity_falls_in_five_levels_against_the_busiest_day() {
    assert_eq!(HeatLevel::of(0, 10), HeatLevel::NONE);
    assert_eq!(HeatLevel::of(5, 0), HeatLevel::NONE);
    assert_eq!(HeatLevel::of(1, 10).index(), 1);
    assert_eq!(HeatLevel::of(3, 10).index(), 2);
    assert_eq!(HeatLevel::of(6, 10).index(), 3);
    assert_eq!(HeatLevel::of(10, 10).index(), 4);
    assert_eq!(HeatLevel::of(u64::MAX, u64::MAX).index(), 4);
    assert_eq!(HeatLevel::ALL.len(), HEAT_LEVELS);
}

#[test]
fn the_metric_switch_reads_another_number_of_the_same_days() {
    let today = date("2026-11-09");
    let mut activity = KraddieActivity::default();
    activity.record(date("2026-11-02"), &runs(2));
    activity.record(
        date("2026-11-03"),
        &KraddieDay {
            runs: 1,
            lines_added: 40,
            lines_removed: 10,
            spend: Spend {
                tokens: TokenUsage {
                    input: 900,
                    output: 100,
                    ..TokenUsage::default()
                },
                cost: UsdCost::from_nanos(25_000_000),
            },
            ..KraddieDay::default()
        },
    );
    activity.record(date("2025-01-01"), &runs(9));
    let by_runs = HeatMap::build(&activity, today, HeatMetric::Runs);
    assert_eq!((by_runs.total, by_runs.max), (3, 2));
    let lines = HeatMap::build(&activity, today, HeatMetric::LinesChanged);
    assert_eq!((lines.total, lines.max), (50, 50));
    let tokens = HeatMap::build(&activity, today, HeatMetric::Tokens);
    assert_eq!(tokens.total, 1000);
    let cost = HeatMap::build(&activity, today, HeatMetric::Cost);
    assert_eq!(cost.total, 25_000_000);
    let cell = |map: &HeatMap, when: &str| {
        map.weeks
            .iter()
            .flatten()
            .flatten()
            .find(|cell| cell.date == date(when))
            .copied()
            .unwrap_or_else(|| unreachable!())
    };
    assert_eq!(cell(&by_runs, "2026-11-02").level.index(), 4);
    assert_eq!(cell(&by_runs, "2026-11-03").level.index(), 2);
    assert_eq!(cell(&lines, "2026-11-02").level, HeatLevel::NONE);
    assert_eq!(cell(&lines, "2026-11-03").day.lines_added, 40);
}

#[test]
fn months_are_labelled_where_their_first_week_starts() {
    let map = HeatMap::build(
        &KraddieActivity::default(),
        date("2026-11-09"),
        HeatMetric::Runs,
    );
    let names: Vec<&str> = map.months.iter().map(MonthLabel::short_name).collect();
    assert_eq!(names, vec!["May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov"]);
    let weeks: Vec<usize> = map.months.iter().map(|label| label.week).collect();
    assert_eq!(weeks.first(), Some(&0));
    assert!(weeks.windows(2).all(|pair| pair[1] > pair[0]));
    let crowded = HeatMap::build(
        &KraddieActivity::default(),
        date("2026-11-28"),
        HeatMetric::Runs,
    );
    let first = crowded.months.first().unwrap_or_else(|| unreachable!());
    assert_eq!(first.short_name(), "Jun");
}

#[test]
fn metrics_have_labels_for_the_switch() {
    let labels: Vec<&str> = HeatMetric::ALL
        .iter()
        .map(|metric| metric.label())
        .collect();
    assert_eq!(labels, vec!["Runs", "Lines changed", "Tokens", "Cost"]);
}
