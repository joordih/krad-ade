use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, linear_color_stop, linear_gradient, px, relative, rgb, transparent_black, Animation,
    AnimationExt as _, AnyElement, App, Div, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, Rgba, Stateful, Styled,
};
use krad_domain::{AgentPreset, AgentState, TerminalId};
use std::time::Duration;

use crate::components::{harness_icon, icon, IconName, StatusKind};
use crate::theme::{
    palette, row_fill, row_shadow, Palette, ICON_SM, LINE_SM, SPACE_1, SPACE_1_5, TAB_H, TEXT_SM,
};

const TAB_MIN_W: f32 = 110.0;
pub(super) const TAB_MAX_W: f32 = 200.0;
const TAB_RADIUS: f32 = 8.0;
const BADGE: f32 = 20.0;
const BADGE_RADIUS: f32 = 6.0;
const BADGE_GLYPH: f32 = 13.0;
const BADGE_GLYPH_ON_DARK: u32 = 0x00ff_ffff;
const BADGE_GLYPH_ON_LIGHT: u32 = 0x001c_1c1e;
const LIGHT_FILL_LUMINANCE: f32 = 0.6;
const LUMA_RED: f32 = 0.2126;
const LUMA_GREEN: f32 = 0.7152;
const LUMA_BLUE: f32 = 0.0722;
const STATE_DOT: f32 = 8.0;
const STATE_DOT_RING: f32 = 2.0;
const STATE_DOT_OUTSET: f32 = 3.0;
const SWEEP_H: f32 = 2.0;
const SWEEP_INSET: f32 = 6.0;
const SWEEP_W: f32 = 120.0;
const SWEEP_PERIOD: Duration = Duration::from_millis(1_400);
const SWEEP_FPS: f32 = 60.0;
const SWEEP_ANGLE: f32 = 90.0;
const SWEEP_ID: &str = "session-tab-sweep";
pub(super) const TAB_HOVER_GROUP: &str = "session-tab-hover";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BadgeLook {
    Filled { glyph: IconName, fill: u32 },
    Neutral(IconName),
    Bare(IconName),
}

impl BadgeLook {
    pub(super) fn of(preset: AgentPreset, leads: bool, is_file: bool, pal: &Palette) -> Self {
        if is_file {
            return Self::Bare(IconName::File);
        }
        let (glyph, fill) = harness_icon(preset, pal);
        let glyph = if leads { IconName::Crown } else { glyph };
        if preset == AgentPreset::Shell {
            Self::Neutral(glyph)
        } else {
            Self::Filled { glyph, fill }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum StateDot {
    Waiting,
    Done,
    Failed,
}

impl StateDot {
    pub(super) const fn of(kind: StatusKind) -> Option<Self> {
        match kind {
            StatusKind::Agent(AgentState::WaitingForInput) => Some(Self::Waiting),
            StatusKind::Agent(AgentState::Done) => Some(Self::Done),
            StatusKind::Failed => Some(Self::Failed),
            StatusKind::Agent(AgentState::Idle | AgentState::Working) => None,
        }
    }

    const fn color(self, pal: &Palette) -> u32 {
        match self {
            Self::Waiting => pal.warning_fg,
            Self::Done => pal.success_fg,
            Self::Failed => pal.error_fg,
        }
    }
}

fn glyph_on(fill: u32) -> u32 {
    let color = rgb(fill);
    let luminance = LUMA_BLUE.mul_add(color.b, LUMA_GREEN.mul_add(color.g, LUMA_RED * color.r));
    if luminance > LIGHT_FILL_LUMINANCE {
        BADGE_GLYPH_ON_LIGHT
    } else {
        BADGE_GLYPH_ON_DARK
    }
}

#[derive(Clone, Copy)]
struct DotRing {
    rest: Rgba,
    hover: Rgba,
}

impl DotRing {
    fn of(shown: bool, pal: &Palette) -> Self {
        if shown {
            let tile = rgb(pal.bg_row_selected);
            return Self {
                rest: tile,
                hover: tile,
            };
        }
        let rest = pal.bg_hover.over(rgb(pal.bg_surface));
        Self {
            rest,
            hover: pal.bg_hover.over(rest),
        }
    }
}

pub(super) fn tab_tile(id: impl Into<ElementId>, shown: bool, cx: &App) -> Stateful<Div> {
    let pal = palette(cx);
    div()
        .id(id)
        .group(TAB_HOVER_GROUP)
        .relative()
        .flex()
        .flex_row()
        .flex_1()
        .min_w(px(TAB_MIN_W))
        .max_w(px(TAB_MAX_W))
        .items_center()
        .gap(px(SPACE_1_5))
        .h(px(TAB_H))
        .px(px(SPACE_1))
        .rounded(px(TAB_RADIUS))
        .cursor_pointer()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(if shown {
            pal.text_strong
        } else {
            pal.text_muted
        }))
        .border_1()
        .border_color(transparent_black())
        .when(shown, |tab| {
            tab.font_weight(FontWeight::MEDIUM)
                .bg(row_fill(pal, cx))
                .border_color(pal.border)
                .shadow(row_shadow(pal, cx))
        })
        .when(!shown, |tab| {
            tab.hover(|style| style.bg(pal.bg_hover).text_color(rgb(pal.text_default)))
        })
}

pub(super) fn tab_badge(
    look: BadgeLook,
    dot: Option<StateDot>,
    shown: bool,
    pal: &Palette,
) -> AnyElement {
    let badge = div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(BADGE))
        .rounded(px(BADGE_RADIUS));
    let badge = match look {
        BadgeLook::Filled { glyph, fill } => {
            badge
                .bg(rgb(fill))
                .child(icon(glyph, BADGE_GLYPH, glyph_on(fill)))
        }
        BadgeLook::Neutral(glyph) => {
            badge
                .bg(rgb(pal.chip_bg))
                .child(icon(glyph, BADGE_GLYPH, pal.text_strong))
        }
        BadgeLook::Bare(glyph) => badge.child(icon(glyph, ICON_SM, pal.text_muted)),
    };
    badge
        .when_some(dot, |badge, dot| {
            badge.child(state_dot(dot, DotRing::of(shown, pal), pal))
        })
        .into_any_element()
}

fn state_dot(dot: StateDot, ring: DotRing, pal: &Palette) -> Div {
    let hover = ring.hover;
    div()
        .absolute()
        .top(px(-STATE_DOT_OUTSET))
        .right(px(-STATE_DOT_OUTSET))
        .size(px(2.0f32.mul_add(STATE_DOT_RING, STATE_DOT)))
        .rounded_full()
        .bg(rgb(dot.color(pal)))
        .border(px(STATE_DOT_RING))
        .border_color(ring.rest)
        .group_hover(TAB_HOVER_GROUP, move |style| style.border_color(hover))
}

pub(super) fn sweep_color(preset: AgentPreset, pal: &Palette) -> u32 {
    harness_icon(preset, pal).1
}

pub(super) fn working_sweep(id: TerminalId, color: u32) -> AnyElement {
    let lit: Hsla = rgb(color).into();
    let clear = Hsla { a: 0.0, ..lit };
    let half = |from: Hsla, to: Hsla| {
        div().flex_1().h_full().bg(linear_gradient(
            SWEEP_ANGLE,
            linear_color_stop(from, 0.0),
            linear_color_stop(to, 1.0),
        ))
    };
    let band = move |phase: f32| {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-SWEEP_W * (1.0 - phase)))
            .w(px(SWEEP_W))
            .flex()
            .flex_row()
            .child(half(clear, lit))
            .child(half(lit, clear))
    };
    div()
        .absolute()
        .bottom_0()
        .left(px(SWEEP_INSET))
        .right(px(SWEEP_INSET))
        .h(px(SWEEP_H))
        .rounded_full()
        .overflow_hidden()
        .child(
            div().absolute().top_0().bottom_0().w_0().with_animation(
                (SWEEP_ID, id.as_u64()),
                Animation::new(SWEEP_PERIOD)
                    .repeat_synced()
                    .with_max_fps(SWEEP_FPS),
                move |runner, phase| runner.left(relative(phase)).child(band(phase)),
            ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Appearance;

    #[test]
    fn agents_get_a_filled_badge_shells_a_neutral_one_and_files_a_bare_glyph() {
        let pal = Appearance::Light.palette();
        let (claude, brand) = harness_icon(AgentPreset::ClaudeCode, pal);
        assert_eq!(
            BadgeLook::of(AgentPreset::ClaudeCode, false, false, pal),
            BadgeLook::Filled {
                glyph: claude,
                fill: brand
            }
        );
        assert_eq!(
            BadgeLook::of(AgentPreset::Shell, false, false, pal),
            BadgeLook::Neutral(IconName::Terminal)
        );
        assert_eq!(
            BadgeLook::of(AgentPreset::ClaudeCode, false, true, pal),
            BadgeLook::Bare(IconName::File)
        );
    }

    #[test]
    fn a_lead_keeps_its_harness_fill_under_the_crown() {
        let pal = Appearance::Light.palette();
        let (_, brand) = harness_icon(AgentPreset::ClaudeCode, pal);
        assert_eq!(
            BadgeLook::of(AgentPreset::ClaudeCode, true, false, pal),
            BadgeLook::Filled {
                glyph: IconName::Crown,
                fill: brand
            }
        );
    }

    #[test]
    fn only_waiting_done_and_failed_tabs_carry_a_dot() {
        let pal = Appearance::Light.palette();
        let dot = |kind| StateDot::of(kind).map(|dot| dot.color(pal));
        assert_eq!(
            dot(StatusKind::Agent(AgentState::WaitingForInput)),
            Some(pal.warning_fg)
        );
        assert_eq!(
            dot(StatusKind::Agent(AgentState::Done)),
            Some(pal.success_fg)
        );
        assert_eq!(dot(StatusKind::Failed), Some(pal.error_fg));
        assert_eq!(dot(StatusKind::Agent(AgentState::Idle)), None);
        assert_eq!(dot(StatusKind::Agent(AgentState::Working)), None);
    }

    #[test]
    fn badge_glyphs_stay_readable_on_light_and_dark_fills() {
        assert_eq!(glyph_on(0x00d9_7757), BADGE_GLYPH_ON_DARK);
        assert_eq!(glyph_on(0x001c_1c1e), BADGE_GLYPH_ON_DARK);
        assert_eq!(glyph_on(0x00f5_f5f7), BADGE_GLYPH_ON_LIGHT);
    }

    #[test]
    fn the_dot_ring_matches_the_shown_tile_and_darkens_on_hover_elsewhere() {
        let pal = Appearance::Light.palette();
        let shown = DotRing::of(true, pal);
        assert_eq!(shown.rest, rgb(pal.bg_row_selected));
        assert_eq!(shown.rest, shown.hover);
        let other = DotRing::of(false, pal);
        assert_ne!(other.rest, other.hover);
    }
}
