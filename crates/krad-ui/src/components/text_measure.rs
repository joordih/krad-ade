use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{font, px, App, FontWeight};

const FALLBACK_ADVANCE: f32 = 0.62;
const MEASURE_SLACK: f32 = 2.0;

#[must_use]
pub(crate) fn label_width(text: &str, size: f32, weight: FontWeight, cx: &App) -> f32 {
    let system = cx.text_system();
    let family = cx.theme().font_family.clone();
    let font_id = system.resolve_font(&gpui_kit::Font {
        weight,
        ..font(family)
    });
    let fallback = size * FALLBACK_ADVANCE;
    let advances: f32 = text
        .chars()
        .map(|ch| {
            system
                .advance(font_id, px(size), ch)
                .map_or(fallback, |advance| advance.width.as_f32())
        })
        .sum();
    advances + MEASURE_SLACK
}
