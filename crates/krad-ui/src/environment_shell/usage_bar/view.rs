use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use std::time::SystemTime;

use super::model::{usage_block, BlockInput, Cell, Meter, Tone, UsageBlock};
use super::EnvironmentShell;
use crate::components::{harness_icon, icon, tooltip_view, Button, IconName};
use crate::theme::{
    palette, Palette, ICON_XS, LINE_2XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_5, TEXT_2XS,
    WINDOW_INSET,
};

pub(in crate::environment_shell) const USAGE_BAR_H: f32 = 26.0;
const METER_W: f32 = 40.0;
const METER_H: f32 = 4.0;
const BLOCK_PREFIX: &str = "usage-bar-block";
const REFRESH_ID: &str = "usage-bar-refresh";
const REFRESH_TOOLTIP: &str = "Refresh usage";
const CELL_SEPARATOR: &str = "·";
const FULL_SHARE: f32 = 100.0;

const fn tone_color(tone: Tone, pal: &Palette) -> u32 {
    match tone {
        Tone::Calm => pal.accent,
        Tone::Warn => pal.attention,
        Tone::Limit => pal.error_fg,
    }
}

const fn cell_color(tone: Tone, stale: bool, pal: &Palette) -> u32 {
    match (tone, stale) {
        (_, true) => pal.text_disabled,
        (Tone::Calm, false) => pal.text_muted,
        (Tone::Warn, false) => pal.attention_text,
        (Tone::Limit, false) => pal.error_fg,
    }
}

fn meter(meter: &Meter, stale: bool, pal: &Palette) -> AnyElement {
    let fill = if stale {
        pal.text_disabled
    } else {
        tone_color(meter.tone, pal)
    };
    let track: Hsla = pal.divider.into();
    let share = f32::from(meter.used) / FULL_SHARE;
    div()
        .flex_none()
        .w(px(METER_W))
        .h(px(METER_H))
        .rounded_full()
        .bg(track)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(px(METER_W * share))
                .rounded_full()
                .bg(rgb(fill)),
        )
        .into_any_element()
}

fn cells(cells: &[Cell], stale: bool, pal: &Palette) -> Vec<AnyElement> {
    let mut shown = Vec::with_capacity(cells.len() * 2);
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            shown.push(
                div()
                    .text_color(rgb(pal.text_disabled))
                    .child(CELL_SEPARATOR)
                    .into_any_element(),
            );
        }
        shown.push(
            div()
                .text_color(rgb(cell_color(cell.tone, stale, pal)))
                .child(cell.text.clone())
                .into_any_element(),
        );
    }
    shown
}

fn block_view(index: usize, block: UsageBlock, pal: &Palette) -> AnyElement {
    let (glyph, tint) = harness_icon(block.harness, pal);
    let tooltip = SharedString::from(block.tooltip);
    div()
        .id((BLOCK_PREFIX, index))
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(SPACE_1_5))
        .text_size(px(TEXT_2XS))
        .line_height(px(LINE_2XS))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx))
        .child(icon(glyph, ICON_XS, tint))
        .child(meter(&block.meter, block.stale, pal))
        .children(cells(&block.cells, block.stale, pal))
        .children(
            block
                .status
                .map(|status| div().text_color(rgb(pal.text_subtle)).child(status)),
        )
        .into_any_element()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn usage_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.usage_bar_shown() {
            return None;
        }
        let pal = palette(cx);
        let link = self.active_link();
        let host = link.host();
        let now = SystemTime::now();
        let warn_at = self.settings.plan_usage.warn_at;
        let blocks: Vec<AnyElement> = self
            .usage_harnesses(&link)
            .into_iter()
            .enumerate()
            .map(|(index, harness)| {
                let key = (host.clone(), harness);
                let input = BlockInput {
                    harness,
                    reading: self.usage.readings.get(&key),
                    note: self.usage.notes.get(&key),
                };
                block_view(index, usage_block(input, warn_at, now), pal)
            })
            .collect();
        if blocks.is_empty() {
            return None;
        }
        let refresh = Button::new(REFRESH_ID)
            .icon(IconName::Refresh)
            .small()
            .tooltip(REFRESH_TOOLTIP)
            .on_click(cx.listener(|this: &mut Self, _: &ClickEvent, _, cx| {
                this.refresh_usage(cx);
            }));
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .h(px(USAGE_BAR_H))
                .px(px(WINDOW_INSET + SPACE_2))
                .pb(px(SPACE_1))
                .gap(px(SPACE_5))
                .overflow_hidden()
                .children(blocks)
                .child(refresh)
                .into_any_element(),
        )
    }
}
