use gpui_kit::{
    canvas, div, point, px, rgb, AnyElement, Bounds, Hsla, InteractiveElement, IntoElement,
    ParentElement, PathBuilder, Pixels, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::TerminalId;
use std::f32::consts::{FRAC_PI_2, TAU};

use super::super::facts::Facts;
use crate::components::tooltip_view;
use crate::theme::{Palette, SPACE_1};

const WARN_PERCENT: u16 = 80;
const USAGE_PREFIX: &str = "surface-usage";
const CONTEXT_LABEL: &str = "Context";
const FIVE_HOUR_LABEL: &str = "5h";
const SEVEN_DAY_LABEL: &str = "7d";
const PERCENT_SIGN: &str = "%";
const USAGE_SEPARATOR: &str = " · ";
const MILLION: u64 = 1_000_000;
const THOUSAND: u64 = 1_000;
const MILLION_SUFFIX: &str = "M";
const THOUSAND_SUFFIX: &str = "K";
const FULL_PERCENT: u16 = 100;
const RING_SIZE: f32 = 16.0;
const RING_STROKE: f32 = 2.0;
const RING_SEGMENTS: u16 = 48;

pub(super) fn window_label(tokens: u64) -> String {
    if tokens >= MILLION && tokens.is_multiple_of(MILLION) {
        format!("{}{MILLION_SUFFIX}", tokens / MILLION)
    } else if tokens >= THOUSAND {
        format!("{}{THOUSAND_SUFFIX}", tokens / THOUSAND)
    } else {
        tokens.to_string()
    }
}

fn usage_text(facts: &Facts) -> Option<String> {
    let parts: Vec<String> = [
        (CONTEXT_LABEL, facts.context),
        (FIVE_HOUR_LABEL, facts.usage.five_hour),
        (SEVEN_DAY_LABEL, facts.usage.seven_day),
    ]
    .into_iter()
    .filter_map(|(label, used)| Some(format!("{label} {}{PERCENT_SIGN}", used?)))
    .collect();
    (!parts.is_empty()).then(|| parts.join(USAGE_SEPARATOR))
}

pub(super) fn usage_ring(id: TerminalId, facts: &Facts, pal: &Palette) -> Option<AnyElement> {
    let text = SharedString::from(usage_text(facts)?);
    let used = facts.context.unwrap_or_default().min(FULL_PERCENT);
    let warns = [facts.context, facts.usage.five_hour, facts.usage.seven_day]
        .into_iter()
        .flatten()
        .any(|used| used >= WARN_PERCENT);
    let fill: Hsla = rgb(if warns {
        pal.attention_text
    } else {
        pal.text_muted
    })
    .into();
    let track: Hsla = pal.divider.into();
    let fraction = f32::from(used) / f32::from(FULL_PERCENT);
    Some(
        div()
            .id((USAGE_PREFIX, id.as_u64()))
            .flex_none()
            .size(px(RING_SIZE))
            .ml(px(SPACE_1))
            .tooltip(move |_, cx| tooltip_view(text.clone(), cx))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| {
                        paint_ring(bounds, 1.0, track, window);
                        paint_ring(bounds, fraction, fill, window);
                    },
                )
                .size_full(),
            )
            .into_any_element(),
    )
}

fn paint_ring(bounds: Bounds<Pixels>, fraction: f32, color: Hsla, window: &mut Window) {
    if fraction <= 0.0 {
        return;
    }
    let size = bounds.size.width.as_f32().min(bounds.size.height.as_f32());
    let radius = (size - RING_STROKE) / 2.0;
    let (cx, cy) = (
        bounds.origin.x.as_f32() + size / 2.0,
        bounds.origin.y.as_f32() + size / 2.0,
    );
    let at = |turn: f32| {
        let angle = turn * TAU - FRAC_PI_2;
        point(px(cx + radius * angle.cos()), px(cy + radius * angle.sin()))
    };
    let mut ring = PathBuilder::stroke(px(RING_STROKE));
    ring.move_to(at(0.0));
    for step in 1..=RING_SEGMENTS {
        ring.line_to(at(fraction * f32::from(step) / f32::from(RING_SEGMENTS)));
    }
    if let Ok(path) = ring.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_context_window_reads_in_millions_or_thousands() {
        assert_eq!(window_label(1_000_000), "1M");
        assert_eq!(window_label(200_000), "200K");
        assert_eq!(window_label(1_500_000), "1500K");
        assert_eq!(window_label(512), "512");
    }
}
