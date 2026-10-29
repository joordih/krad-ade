use gpui_kit::{
    div, px, rgb, Animation, AnimationExt as _, AnyElement, ElementId, HighlightStyle, Hsla,
    IntoElement, ParentElement, SharedString, Styled, StyledText,
};
use std::ops::Range;
use std::time::{Duration, Instant};

use crate::theme::{Palette, LINE_SM, LINE_XS, TEXT_SM, TEXT_XS};

const DOTS_GRID: u8 = 3;
const DOT_SIZE: f32 = 4.0;
const DOT_GAP: f32 = 2.0;
const DOT_RADIUS: f32 = 1.0;
const WAVE_PERIOD: Duration = Duration::from_millis(640);
const WAVE_TRAIL: f32 = 0.3;
const WAVE_FALLOFF: f32 = 1.5;
const DOT_MIN_OPACITY: f32 = 0.12;
const SHIMMER_PERIOD: Duration = Duration::from_millis(1800);
const SHIMMER_HALF_W: f32 = 0.25;
const SHIMMER_BASE_ALPHA: f32 = 0.5;
const MOTION_FPS: f32 = 30.0;
const INDICATOR_GAP: f32 = 10.0;
const DOTS_ID: &str = "thinking-dots";
const LABEL_ID: &str = "thinking-label";

fn share(index: usize, count: usize) -> f32 {
    let as_f32 = |value: usize| u16::try_from(value).map_or(f32::from(u16::MAX), f32::from);
    (as_f32(index) + 0.5) / as_f32(count.max(1))
}

fn wave_opacity(col: u8, row: u8, phase: f32) -> f32 {
    let last = f32::from(DOTS_GRID - 1);
    let grid = f32::from(DOTS_GRID);
    let front = f32::from(col + row) / (2.0 * last) * (grid / (grid + 1.0));
    let behind = (phase - front).rem_euclid(1.0);
    let lit = (1.0 - behind / WAVE_TRAIL).max(0.0).powf(WAVE_FALLOFF);
    DOT_MIN_OPACITY + (1.0 - DOT_MIN_OPACITY) * lit
}

fn shimmer_highlights(label: &str, phase: f32, color: u32) -> Vec<(Range<usize>, HighlightStyle)> {
    let count = label.chars().count();
    let center = phase * 2.0f32.mul_add(SHIMMER_HALF_W, 1.0) - SHIMMER_HALF_W;
    label
        .char_indices()
        .enumerate()
        .map(|(index, (start, character))| {
            let glow = (1.0 - (share(index, count) - center).abs() / SHIMMER_HALF_W).max(0.0);
            let mut tint: Hsla = rgb(color).into();
            tint.a = (1.0 - SHIMMER_BASE_ALPHA).mul_add(glow, SHIMMER_BASE_ALPHA);
            let style = HighlightStyle {
                color: Some(tint),
                ..HighlightStyle::default()
            };
            (start..start + character.len_utf8(), style)
        })
        .collect()
}

fn wave_dots(key: u64, color: u32) -> impl IntoElement {
    let wave = Animation::new(WAVE_PERIOD)
        .repeat_synced()
        .with_max_fps(MOTION_FPS);
    div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(px(DOT_GAP))
        .with_animation((DOTS_ID, key), wave, move |grid, phase| {
            grid.children((0..DOTS_GRID).map(|row| {
                div()
                    .flex()
                    .flex_row()
                    .gap(px(DOT_GAP))
                    .children((0..DOTS_GRID).map(|col| {
                        div()
                            .size(px(DOT_SIZE))
                            .rounded(px(DOT_RADIUS))
                            .bg(rgb(color))
                            .opacity(wave_opacity(col, row, phase))
                    }))
            }))
        })
}

pub(crate) fn shimmer_text(
    id: impl Into<ElementId>,
    label: SharedString,
    color: u32,
) -> impl IntoElement {
    let sweep = Animation::new(SHIMMER_PERIOD)
        .repeat_synced()
        .with_max_fps(MOTION_FPS);
    div()
        .min_w_0()
        .truncate()
        .with_animation(id, sweep, move |text, phase| {
            text.child(
                StyledText::new(label.clone())
                    .with_highlights(shimmer_highlights(&label, phase, color)),
            )
        })
}

pub(crate) fn thinking_indicator(
    key: u64,
    label: SharedString,
    started: Instant,
    mono: SharedString,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(INDICATOR_GAP))
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .child(wave_dots(key, pal.text_muted))
        .child(shimmer_text((LABEL_ID, key), label, pal.text_strong))
        .child(
            div()
                .flex_none()
                .font_family(mono)
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(format!("{:.1}s", started.elapsed().as_secs_f32())),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_lights_the_dots_its_front_reaches_and_dims_the_rest() {
        assert!((wave_opacity(0, 0, 0.0) - 1.0).abs() < f32::EPSILON);
        assert!((wave_opacity(1, 1, 0.0) - DOT_MIN_OPACITY).abs() < f32::EPSILON);
        let front_at_corner = f32::from(DOTS_GRID) / f32::from(DOTS_GRID + 1);
        assert!(wave_opacity(2, 2, front_at_corner) > wave_opacity(0, 0, front_at_corner));
    }

    #[test]
    fn the_shimmer_brightens_the_characters_under_it_only() {
        let alpha = |phase: f32| -> Vec<f32> {
            shimmer_highlights("Thinking", phase, 0)
                .iter()
                .filter_map(|(_, style)| style.color.map(|color| color.a))
                .collect()
        };
        let middle = alpha(0.5);
        assert_eq!(middle.len(), "Thinking".len());
        assert!(middle[4] > middle[0]);
        assert!(alpha(0.0)
            .iter()
            .all(|value| (value - SHIMMER_BASE_ALPHA).abs() < f32::EPSILON));
    }
}
