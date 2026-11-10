use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    canvas, div, fill, point, px, rgb, rgba, size, Animation, AnimationExt as _, AnyElement, App,
    Bounds, Context, InteractiveElement, IntoElement, MouseMoveEvent, ParentElement, Pixels, Rgba,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{HeatCell, HeatLevel, HeatMap, HeatMetric, KraddieId, UsdCost, DAYS_PER_WEEK};
use std::cell::Cell;
use std::rc::Rc;

use super::super::super::spending::token_count;
use super::super::super::EnvironmentShell;
use super::super::layout::{PageLayout, HEAT_GAP, HEAT_WEEKDAY_W};
use super::super::motion::{fading_in, wave_reveal, HEAT_WAVE};
use super::super::segmented::{sliding_track, sliding_width, Sliding};
use super::super::tile::caption;
use crate::components::Stretch;
use crate::theme::{palette, Palette, LINE_XS, RADIUS_XS, SPACE_1, SPACE_2, TEXT_2XS};

const GAP: f32 = HEAT_GAP;
const CELL_ROUNDING: f32 = 0.22;
const HOVER_RING: f32 = 1.5;
const MONTH_H: f32 = 16.0;
const TOOLTIP_W: f32 = 220.0;
const TOOLTIP_LIFT: f32 = 30.0;
const LEVEL_ALPHA: [u32; 4] = [0x4D, 0x80, 0xB3, 0xFF];
const ALPHA_SHIFT: u32 = 8;
const WEEKDAY_LABELS: [(usize, &str); 3] = [(1, "Mon"), (3, "Wed"), (5, "Fri")];
const QUIET: &str = "Quiet";
const BUSY: &str = "Busy";
const IN_RANGE: &str = "over six months";
const DAY_FORMAT: &str = "%A, %-d %B %Y";
const METRIC_ID: &str = "kraddie-heat-metric";
const METRIC_SHORT: [&str; 4] = ["Runs", "Lines", "Tokens", "Cost"];
const METRIC_MENU: &str = "kraddie-heat-metric";
pub(super) const TITLE_ROOM: f32 = 300.0;
const GRID_ID: &str = "kraddie-heat-grid";
const SCROLL_ID: &str = "kraddie-heat-scroll";
const WAVE_ID: &str = "kraddie-heat-wave";
const TIP_ID: &str = "kraddie-heat-tip";
const RUNS_UNIT: &str = "runs";
const RUN_UNIT: &str = "run";
const LINES_UNIT: &str = "lines changed";
const TOKENS_UNIT: &str = "tokens";
const ON_DAY: &str = "on";
const REVEALED: f32 = 1.0;
const TIP_DAY_BITS: u32 = u8::BITS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct HeatHover {
    week: usize,
    day: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HeatGeometry {
    weeks: usize,
    cell: f32,
}

impl HeatGeometry {
    pub(super) const fn new(weeks: usize, cell: f32) -> Self {
        Self { weeks, cell }
    }

    fn step(self) -> f32 {
        self.cell + GAP
    }

    #[allow(clippy::cast_precision_loss)]
    fn grid_width(self) -> f32 {
        (self.weeks as f32 * self.step() - GAP).max(0.0)
    }

    #[allow(clippy::cast_precision_loss)]
    fn grid_height(self) -> f32 {
        DAYS_PER_WEEK as f32 * self.step() - GAP
    }

    #[allow(clippy::cast_precision_loss)]
    fn offset(self, index: usize) -> f32 {
        index as f32 * self.step()
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(super) fn hit(self, x: f32, y: f32) -> Option<HeatHover> {
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (week, day) = ((x / self.step()) as usize, (y / self.step()) as usize);
        let inside = x - self.offset(week) < self.cell && y - self.offset(day) < self.cell;
        (inside && week < self.weeks && day < DAYS_PER_WEEK).then_some(HeatHover { week, day })
    }
}

pub(super) fn metric_amount(metric: HeatMetric, value: u64) -> String {
    match metric {
        HeatMetric::Runs if value == 1 => format!("{value} {RUN_UNIT}"),
        HeatMetric::Runs => format!("{value} {RUNS_UNIT}"),
        HeatMetric::LinesChanged => format!("{value} {LINES_UNIT}"),
        HeatMetric::Tokens => format!("{} {TOKENS_UNIT}", token_count(value)),
        HeatMetric::Cost => UsdCost::from_nanos(value).to_string(),
    }
}

fn day_detail(metric: HeatMetric, cell: &HeatCell) -> String {
    format!(
        "{} {ON_DAY} {}",
        metric_amount(metric, cell.value),
        cell.date.format(DAY_FORMAT)
    )
}

fn level_color(level: HeatLevel, pal: &Palette) -> Rgba {
    match level
        .index()
        .checked_sub(1)
        .and_then(|step| LEVEL_ALPHA.get(step))
    {
        Some(alpha) => rgba((pal.accent << ALPHA_SHIFT) | alpha),
        None => rgb(pal.chip_bg),
    }
}

struct Painting {
    map: Rc<HeatMap>,
    geometry: HeatGeometry,
    hover: Option<HeatHover>,
    wave: Rc<Cell<f32>>,
}

fn with_alpha(color: Rgba, shown: f32) -> Rgba {
    Rgba {
        a: color.a * shown,
        ..color
    }
}

fn paint_cells(painting: &Painting, origin: Bounds<Pixels>, pal: &Palette, window: &mut Window) {
    let Painting {
        map,
        geometry,
        hover,
        wave,
    } = painting;
    let colors = HeatLevel::ALL.map(|level| level_color(level, pal));
    let (left, top) = (origin.origin.x.as_f32(), origin.origin.y.as_f32());
    let progress = wave.get();
    let weeks = map.weeks.len();
    let radius = px(geometry.cell * CELL_ROUNDING);
    for (week, days) in map.weeks.iter().enumerate() {
        let shown = wave_reveal(progress, week, weeks);
        if shown <= 0.0 {
            continue;
        }
        for (day, cell) in days.iter().enumerate() {
            let Some(cell) = cell else {
                continue;
            };
            let at = point(
                px(left + geometry.offset(week)),
                px(top + geometry.offset(day)),
            );
            let color = colors.get(cell.level.index()).copied().unwrap_or(colors[0]);
            let side = geometry.cell * shown;
            let inset = (geometry.cell - side) / 2.0;
            let at = point(at.x + px(inset), at.y + px(inset));
            window.paint_quad(
                fill(
                    Bounds::new(at, size(px(side), px(side))),
                    with_alpha(color, shown),
                )
                .corner_radii(radius),
            );
        }
    }
    let Some(HeatHover { week, day }) = *hover else {
        return;
    };
    let at = point(
        px(left + geometry.offset(week) - HOVER_RING),
        px(top + geometry.offset(day) - HOVER_RING),
    );
    let side = px(geometry.cell + 2.0 * HOVER_RING);
    window.paint_quad(
        gpui_kit::outline(
            Bounds::new(at, size(side, side)),
            rgb(pal.text_strong),
            gpui_kit::BorderStyle::Solid,
        )
        .corner_radii(radius + px(HOVER_RING)),
    );
}

fn small_label(text: &'static str, pal: &Palette) -> gpui_kit::Div {
    div()
        .text_size(px(TEXT_2XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(text)
}

impl EnvironmentShell {
    pub(super) fn heat_map_block(
        &self,
        id: &KraddieId,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let map = self.kraddies.profile.heat_map(id);
        let heat = layout.heat(map.weeks.len());
        let geometry = HeatGeometry::new(map.weeks.len(), heat.cell);
        let total = format!("{} {IN_RANGE}", metric_amount(map.metric, map.total));
        let grid = self.heat_grid(&map, geometry, pal, cx);
        let grid = if heat.scrolls {
            div()
                .id(SCROLL_ID)
                .w_full()
                .overflow_x_scroll()
                .child(grid)
                .into_any_element()
        } else {
            grid
        };
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(grid)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(SPACE_2))
                    .child(caption(total, pal))
                    .child(Self::heat_legend(geometry.cell, pal)),
            )
            .into_any_element()
    }

    #[must_use]
    pub(super) fn metric_switch_width(cx: &App) -> f32 {
        sliding_width(&HeatMetric::ALL.map(HeatMetric::label), cx)
    }

    pub(super) fn metric_switch(&self, container: f32, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.kraddies.profile.metric;
        let labels = HeatMetric::ALL.map(HeatMetric::label);
        let chosen = HeatMetric::ALL
            .iter()
            .position(|metric| *metric == current)
            .unwrap_or_default();
        let sliding = Sliding {
            id: METRIC_ID,
            labels: &labels,
            short: &METRIC_SHORT,
            chosen,
            container,
            stretch: Stretch::Natural,
            menu_open: self.kraddies.profile.open_menu == Some(METRIC_MENU),
        };
        sliding_track(
            &sliding,
            pal,
            cx,
            |index, segment| {
                let metric = HeatMetric::ALL[index];
                segment.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.kraddies.profile.metric = metric;
                    this.kraddies.profile.hover = None;
                    this.kraddies.profile.open_menu = None;
                    cx.notify();
                }))
            },
            |button| {
                button.on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.kraddies.profile.toggle_menu(METRIC_MENU);
                    cx.notify();
                }))
            },
        )
    }

    fn heat_grid(
        &self,
        map: &Rc<HeatMap>,
        geometry: HeatGeometry,
        pal: &'static Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let origin = Rc::clone(&self.kraddies.profile.grid_origin);
        let wave = Rc::new(Cell::new(REVEALED));
        let painting = Painting {
            map: Rc::clone(map),
            geometry,
            hover: self.kraddies.profile.hover,
            wave: Rc::clone(&wave),
        };
        let seen = Rc::clone(&origin);
        let cells = canvas(
            move |bounds, _, _| seen.set(Some(bounds)),
            move |bounds, (), window, _: &mut App| paint_cells(&painting, bounds, pal, window),
        )
        .size_full();
        let hovered = self.kraddies.profile.hover.and_then(|hover| {
            let cell = map
                .weeks
                .get(hover.week)?
                .get(hover.day)
                .copied()
                .flatten()?;
            Some((hover, cell))
        });
        let tooltip = hovered.map(|(hover, cell)| {
            let left = (HEAT_WEEKDAY_W + geometry.offset(hover.week) - TOOLTIP_W / 2.0).clamp(
                0.0,
                (HEAT_WEEKDAY_W + geometry.grid_width() - TOOLTIP_W).max(0.0),
            );
            let tip = div()
                .absolute()
                .left(px(left))
                .top(px(MONTH_H + geometry.offset(hover.day) - TOOLTIP_LIFT))
                .w(px(TOOLTIP_W))
                .px(px(SPACE_2))
                .py(px(SPACE_1))
                .rounded(px(RADIUS_XS))
                .bg(rgb(pal.text_strong))
                .text_color(rgb(pal.bg_surface))
                .text_size(px(TEXT_2XS))
                .line_height(px(LINE_XS))
                .child(day_detail(map.metric, &cell));
            let key = u64::from(u16::try_from(hover.week).unwrap_or_default()) << TIP_DAY_BITS
                | u64::from(u8::try_from(hover.day).unwrap_or_default());
            fading_in(tip, (TIP_ID, key))
        });
        let tracker = Rc::clone(&origin);
        let wave_key = SharedString::from(format!("{WAVE_ID}-{:?}", map.metric));
        div()
            .id(GRID_ID)
            .relative()
            .flex_none()
            .w(px(HEAT_WEEKDAY_W + geometry.grid_width()))
            .h(px(MONTH_H + geometry.grid_height()))
            .children(map.months.iter().map(|label| {
                small_label(label.short_name(), pal)
                    .absolute()
                    .top_0()
                    .left(px(HEAT_WEEKDAY_W + geometry.offset(label.week)))
            }))
            .children(WEEKDAY_LABELS.map(|(day, label)| {
                small_label(label, pal)
                    .absolute()
                    .left_0()
                    .top(px(MONTH_H + geometry.offset(day) - GAP))
            }))
            .child(
                div()
                    .absolute()
                    .left(px(HEAT_WEEKDAY_W))
                    .top(px(MONTH_H))
                    .w(px(geometry.grid_width()))
                    .h(px(geometry.grid_height()))
                    .child(cells)
                    .with_animation(wave_key, Animation::new(HEAT_WAVE), move |grid, t| {
                        wave.set(t);
                        grid
                    }),
            )
            .on_mouse_move(
                cx.listener(move |this: &mut Self, event: &MouseMoveEvent, _, cx| {
                    let hover = tracker.get().and_then(|bounds| {
                        geometry.hit(
                            event.position.x.as_f32() - bounds.origin.x.as_f32(),
                            event.position.y.as_f32() - bounds.origin.y.as_f32(),
                        )
                    });
                    if this.kraddies.profile.hover != hover {
                        this.kraddies.profile.hover = hover;
                        cx.notify();
                    }
                }),
            )
            .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                if !*hovered && this.kraddies.profile.hover.take().is_some() {
                    cx.notify();
                }
            }))
            .when_some(tooltip, ParentElement::child)
            .into_any_element()
    }

    fn heat_legend(cell: f32, pal: &Palette) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .child(small_label(QUIET, pal))
            .children(HeatLevel::ALL.map(|level| {
                div()
                    .size(px(cell))
                    .rounded(px(cell * CELL_ROUNDING))
                    .bg(level_color(level, pal))
            }))
            .child(small_label(BUSY, pal))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: f32 = 10.0;
    const STEP: f32 = CELL + GAP;

    #[test]
    fn the_pointer_finds_the_day_under_it_and_misses_the_gaps() {
        let geometry = HeatGeometry::new(27, CELL);
        assert_eq!(geometry.hit(1.0, 1.0), Some(HeatHover { week: 0, day: 0 }));
        assert_eq!(
            geometry.hit(STEP * 3.0 + 2.0, STEP * 6.0 + 9.0),
            Some(HeatHover { week: 3, day: 6 })
        );
        assert_eq!(geometry.hit(CELL + 1.0, 1.0), None);
        assert_eq!(geometry.hit(1.0, STEP * 7.0 + 1.0), None);
        assert_eq!(geometry.hit(STEP * 27.0 + 1.0, 1.0), None);
        assert_eq!(geometry.hit(-1.0, 1.0), None);
        assert!((geometry.grid_width() - (27.0 * STEP - GAP)).abs() < f32::EPSILON);
    }

    #[test]
    fn bigger_cells_keep_the_pointer_on_the_same_day() {
        for cell in [9.0, 14.0, 20.0] {
            let geometry = HeatGeometry::new(27, cell);
            let step = cell + GAP;
            assert_eq!(
                geometry.hit(step * 10.0 + cell / 2.0, step * 4.0 + cell / 2.0),
                Some(HeatHover { week: 10, day: 4 })
            );
            assert_eq!(geometry.hit(step * 10.0 + cell + GAP / 2.0, 1.0), None);
        }
    }

    #[test]
    fn amounts_read_in_the_metrics_own_unit() {
        assert_eq!(metric_amount(HeatMetric::Runs, 0), "0 runs");
        assert_eq!(metric_amount(HeatMetric::Runs, 1), "1 run");
        assert_eq!(
            metric_amount(HeatMetric::LinesChanged, 50),
            "50 lines changed"
        );
        assert_eq!(metric_amount(HeatMetric::Cost, 25_000_000), "$0.03");
        assert!(metric_amount(HeatMetric::Tokens, 1500).ends_with(" tokens"));
    }
}
