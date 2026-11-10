use chrono::{DateTime, Local};
use gpui_kit::{
    div, px, rgb, AnimationExt as _, AnyElement, Context, Div, FontWeight, InteractiveElement,
    IntoElement, ParentElement, SharedString, SpringAnimation, Styled,
};
use krad_domain::{AttentionTarget, KraddieDay, KraddieOutcome, KraddieRun, KraddieWake};

use super::super::super::EnvironmentShell;
use super::super::clock::{next_run, next_run_label};
use super::super::layout::{place, CardSpan, PageLayout};
use super::super::motion::{count_steps, counting, entering, SLIDE_SPRING};
use super::super::tile::{caption, grid, tile, tile_inner, Tile};
use super::super::KraddieEntry;
use super::heatmap::TITLE_ROOM;
use super::today;
use crate::components::{Button, IconName};
use crate::theme::{
    palette, Palette, LINE_LG, LINE_SM, LINE_XS, RADIUS_LG, SPACE_0_5, SPACE_2, SPACE_3, TEXT_LG,
    TEXT_SM, TEXT_XS,
};

const GLANCE: [CardSpan; 5] = [
    CardSpan::Full,
    CardSpan::One,
    CardSpan::One,
    CardSpan::One,
    CardSpan::Fill,
];
const TRAIL_TITLE: &str = "Trail";
const TRAIL_NOTE: &str = "Six months of footprints, one square a day";
const DAY_TITLE: &str = "Day so far";
const DAY_NOTE: &str = "Counted since midnight";
const HORIZON_TITLE: &str = "On the horizon";
const FRESH_TITLE: &str = "Fresh from today";
const LOGBOOK_TITLE: &str = "Logbook";
const MERGED_LABEL: &str = "Merged";
const LINES_LABEL: &str = "Lines";
const SESSIONS_LABEL: &str = "Sessions";
const FIRED_LABEL: &str = "Routines fired";
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "\u{2212}";
const NOTHING_PLANNED: &str = "Nothing planned. Give it a routine in Automations.";
const NOTHING_FRESH: &str = "Nothing finished since midnight.";
const NO_RUNS: &str = "No runs logged yet. Wake it up to start one.";
const ROUNDS_LABEL: &str = "Rounds, once things go quiet";
const TIME_FORMAT: &str = "%H:%M";
const LOG_FORMAT: &str = "%a %d %b %H:%M";
const OPEN_PREFIX: &str = "kraddie-log-open";
const FRESH_OPEN_PREFIX: &str = "kraddie-fresh-open";
const OPEN_LABEL: &str = "Open";
const CAPTION_SEPARATOR: &str = " · ";
const TILE_PREFIX: &str = "kraddie-glance";
const STAT_PREFIX: &str = "kraddie-stat";
const ROW_PREFIX: &str = "kraddie-row";
const LOG_HEIGHT_ID: &str = "kraddie-logbook-height";
const LOG_TOGGLE: &str = "kraddie-logbook-toggle";
const SHOW_ALL: &str = "Show all";
const SHOW_FEWER: &str = "Show fewer";
const LOG_COLLAPSED: usize = 4;
const LOG_SHOWN: usize = 12;
const LOG_ROW_H: f32 = 44.0;
const ROW_SUFFIX: &str = "row";

#[derive(Clone, Copy, Debug, PartialEq)]
struct TrailSelector {
    beside: bool,
    container: f32,
}

impl TrailSelector {
    fn place(switch_w: f32, inner: f32) -> Self {
        if switch_w + TITLE_ROOM <= inner {
            return Self {
                beside: true,
                container: inner - TITLE_ROOM,
            };
        }
        Self {
            beside: false,
            container: inner,
        }
    }
}

pub(super) fn ran_today(run: &KraddieRun) -> bool {
    DateTime::<Local>::from(run.started).date_naive() == today()
        && !matches!(run.outcome, KraddieOutcome::Refused { .. })
        && run.wake != KraddieWake::Rounds
}

#[must_use]
fn logbook_height(rows: usize, open: bool) -> f32 {
    let shown = if open {
        rows.min(LOG_SHOWN)
    } else {
        rows.min(LOG_COLLAPSED)
    };
    u16::try_from(shown).map_or(0.0, f32::from) * LOG_ROW_H
}

fn stat(label: &'static str, value: AnyElement, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(SPACE_0_5))
        .p(px(SPACE_3))
        .rounded(px(RADIUS_LG))
        .bg(rgb(pal.bg_surface))
        .border_1()
        .border_color(pal.divider)
        .child(caption(label, pal).truncate())
        .child(value)
        .into_any_element()
}

fn number(color: u32) -> Div {
    div()
        .text_size(px(TEXT_LG))
        .line_height(px(LINE_LG))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(color))
}

fn counted(key: &str, slot: usize, value: u64, sign: &'static str, color: u32) -> AnyElement {
    let frames = count_steps(value)
        .into_iter()
        .map(|step| SharedString::from(format!("{sign}{step}")))
        .collect();
    counting(
        number(color),
        (SharedString::from(format!("{STAT_PREFIX}-{key}")), slot),
        frames,
    )
    .into_any_element()
}

fn empty(text: &'static str, pal: &Palette) -> AnyElement {
    caption(text, pal).py(px(SPACE_2)).into_any_element()
}

fn rows_or(rows: Vec<AnyElement>, text: &'static str, pal: &Palette) -> AnyElement {
    if rows.is_empty() {
        return empty(text, pal);
    }
    div().flex().flex_col().children(rows).into_any_element()
}

fn line(title: SharedString, detail: SharedString, pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .truncate()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_default))
                .child(title),
        )
        .child(caption(detail, pal).truncate())
}

impl EnvironmentShell {
    pub(super) fn glance_grid(
        &self,
        entry: &KraddieEntry,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let id = entry.kraddie.id();
        let spans = place(&GLANCE, layout.columns);
        let width = |index: usize| layout.span_w(spans[index], layout.columns);
        let key = id.to_string();
        let tile_id = |name: &str| SharedString::from(format!("{TILE_PREFIX}-{key}-{name}"));
        let trail = TrailSelector::place(Self::metric_switch_width(cx), tile_inner(width(0)));
        let runs = entry.runs_newest_first();
        let tiles = vec![
            tile(
                Tile {
                    id: tile_id(TRAIL_TITLE),
                    index: 0,
                    width: width(0),
                    title: TRAIL_TITLE,
                    note: Some(TRAIL_NOTE.into()),
                    accessory: trail
                        .beside
                        .then(|| self.metric_switch(trail.container, cx)),
                    body: self.trail_body(&id, trail, layout, cx),
                },
                pal,
            ),
            tile(
                Tile {
                    id: tile_id(DAY_TITLE),
                    index: 1,
                    width: width(1),
                    title: DAY_TITLE,
                    note: Some(DAY_NOTE.into()),
                    accessory: None,
                    body: self.day_stats(entry, pal),
                },
                pal,
            ),
            tile(
                Tile {
                    id: tile_id(HORIZON_TITLE),
                    index: 2,
                    width: width(2),
                    title: HORIZON_TITLE,
                    note: None,
                    accessory: None,
                    body: self.horizon(entry, pal),
                },
                pal,
            ),
            tile(
                Tile {
                    id: tile_id(FRESH_TITLE),
                    index: 3,
                    width: width(3),
                    title: FRESH_TITLE,
                    note: None,
                    accessory: None,
                    body: Self::fresh(entry, &runs, pal, cx),
                },
                pal,
            ),
            tile(
                Tile {
                    id: tile_id(LOGBOOK_TITLE),
                    index: 4,
                    width: width(4),
                    title: LOGBOOK_TITLE,
                    note: None,
                    accessory: self.logbook_toggle(&runs, cx),
                    body: self.logbook(&runs, pal, cx),
                },
                pal,
            ),
        ];
        grid(layout.gutter, tiles).into_any_element()
    }

    fn trail_body(
        &self,
        id: &krad_domain::KraddieId,
        trail: TrailSelector,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let heat = self.heat_map_block(id, layout, cx);
        if trail.beside {
            return heat;
        }
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(self.metric_switch(trail.container, cx))
            .child(heat)
            .into_any_element()
    }

    fn day_stats(&self, entry: &KraddieEntry, pal: &Palette) -> AnyElement {
        let id = entry.kraddie.id();
        let day = self
            .kraddies
            .profile
            .activity_of(&id)
            .map_or_else(KraddieDay::default, |activity| activity.day(today()));
        let KraddieDay {
            prs_merged,
            sessions,
            routines,
            lines_added,
            lines_removed,
            ..
        } = day;
        let key = id.as_str();
        let plain =
            |slot: usize, value: u32| counted(key, slot, u64::from(value), "", pal.text_strong);
        let lines = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_2))
            .child(counted(key, 1, lines_added, ADDED_SIGN, pal.success_fg))
            .child(counted(key, 2, lines_removed, REMOVED_SIGN, pal.error_fg))
            .into_any_element();
        let pair =
            |cards: [AnyElement; 2]| div().flex().flex_row().gap(px(SPACE_2)).children(cards);
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(pair([
                stat(MERGED_LABEL, plain(0, prs_merged), pal),
                stat(LINES_LABEL, lines, pal),
            ]))
            .child(pair([
                stat(SESSIONS_LABEL, plain(3, sessions), pal),
                stat(FIRED_LABEL, plain(4, routines), pal),
            ]))
            .into_any_element()
    }

    fn horizon(&self, entry: &KraddieEntry, pal: &Palette) -> AnyElement {
        let mut upcoming: Vec<(DateTime<Local>, String)> = entry
            .automations
            .iter()
            .filter_map(|routine| {
                let at = next_run(entry, routine, self.kraddies.opened_at)?;
                Some((at, routine.automation.name.to_string()))
            })
            .collect();
        upcoming.sort_by_key(|(at, _)| *at);
        let key = entry.kraddie.id().to_string();
        let scheduled = upcoming.into_iter().map(|(at, name)| {
            (
                SharedString::from(name),
                SharedString::from(next_run_label(&at)),
            )
        });
        let rounds = self.next_check_in(entry).map(|at| {
            let at = DateTime::<Local>::from(at).format(TIME_FORMAT).to_string();
            (SharedString::from(ROUNDS_LABEL), SharedString::from(at))
        });
        let rows = scheduled
            .chain(rounds)
            .enumerate()
            .map(|(index, (title, detail))| {
                let row = line(title, detail, pal).py(px(SPACE_2));
                entering(
                    row,
                    (
                        SharedString::from(format!("{ROW_PREFIX}-horizon-{key}")),
                        index,
                    ),
                    index,
                )
                .into_any_element()
            })
            .collect();
        rows_or(rows, NOTHING_PLANNED, pal)
    }

    fn fresh(
        entry: &KraddieEntry,
        runs: &[&KraddieRun],
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = entry.kraddie.id().to_string();
        let rows = runs
            .iter()
            .copied()
            .filter(|run| ran_today(run))
            .enumerate()
            .map(|(index, run)| {
                let row = Self::run_line(run, index, FRESH_OPEN_PREFIX, TIME_FORMAT, pal, cx);
                entering(
                    row,
                    (
                        SharedString::from(format!("{ROW_PREFIX}-fresh-{key}")),
                        index,
                    ),
                    index,
                )
                .into_any_element()
            })
            .collect();
        rows_or(rows, NOTHING_FRESH, pal)
    }

    fn logbook_toggle(&self, runs: &[&KraddieRun], cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.kraddies.profile.logbook_open;
        (runs.len() > LOG_COLLAPSED).then(|| {
            Button::new(LOG_TOGGLE)
                .label(if open { SHOW_FEWER } else { SHOW_ALL })
                .icon(if open {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                })
                .small()
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.kraddies.profile.logbook_open = !this.kraddies.profile.logbook_open;
                    cx.notify();
                }))
                .into_any_element()
        })
    }

    fn logbook(&self, runs: &[&KraddieRun], pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let runs: Vec<&KraddieRun> = runs.iter().copied().take(LOG_SHOWN).collect();
        if runs.is_empty() {
            return empty(NO_RUNS, pal);
        }
        let height = logbook_height(runs.len(), self.kraddies.profile.logbook_open);
        let rows = runs.into_iter().enumerate().map(|(index, run)| {
            Self::run_line(run, index, OPEN_PREFIX, LOG_FORMAT, pal, cx).h(px(LOG_ROW_H))
        });
        div()
            .flex()
            .flex_col()
            .overflow_hidden()
            .children(rows)
            .with_spring(
                (LOG_HEIGHT_ID, 0usize),
                SpringAnimation::new(SLIDE_SPRING).to(px(height)),
                Styled::h,
            )
            .into_any_element()
    }

    fn run_line(
        run: &KraddieRun,
        index: usize,
        prefix: &'static str,
        time_format: &'static str,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<Div> {
        let at = DateTime::<Local>::from(run.started)
            .format(time_format)
            .to_string();
        let detail = [at.as_str(), run.wake.label()].join(CAPTION_SEPARATOR);
        let open = run.session.clone().map(|link| {
            Button::new((prefix, index))
                .label(OPEN_LABEL)
                .small()
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    let target = AttentionTarget::new(link.environment.clone(), link.terminal);
                    this.open_attention(&target, window, cx);
                }))
        });
        div()
            .id((SharedString::from(format!("{prefix}-{ROW_SUFFIX}")), index))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_LG))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .hover(|style| style.bg(pal.bg_hover))
            .child(line(
                SharedString::from(run.outcome.label().to_owned()),
                detail.into(),
                pal,
            ))
            .children(open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_metric_selector_moves_under_the_title_when_the_trail_is_narrow() {
        let switch_w = 280.0;
        let wide = TrailSelector::place(switch_w, 1200.0);
        assert!(wide.beside);
        assert!(wide.container >= switch_w);
        let narrow = TrailSelector::place(switch_w, 500.0);
        assert!(!narrow.beside);
        assert!((narrow.container - 500.0).abs() < f32::EPSILON);
        for inner in (280_u16..=1600).step_by(40).map(f32::from) {
            let placed = TrailSelector::place(switch_w, inner);
            assert!(placed.container <= inner);
            assert!(!placed.beside || placed.container + TITLE_ROOM <= inner + f32::EPSILON);
        }
    }

    #[test]
    fn the_logbook_opens_to_its_longer_list_and_never_past_its_rows() {
        assert!((logbook_height(10, false) - 4.0 * LOG_ROW_H).abs() < f32::EPSILON);
        assert!((logbook_height(10, true) - 10.0 * LOG_ROW_H).abs() < f32::EPSILON);
        assert!((logbook_height(30, true) - 12.0 * LOG_ROW_H).abs() < f32::EPSILON);
        assert!((logbook_height(2, false) - 2.0 * LOG_ROW_H).abs() < f32::EPSILON);
        assert!(logbook_height(0, true).abs() < f32::EPSILON);
    }
}
