use gpui_kit::{font, px, App};

use super::FamilyKind;

const NARROW: char = 'i';
const WIDE: char = 'W';
const PROBE_PX: f32 = 13.0;
const WIDTH_TOLERANCE: f32 = 0.01;
const DECORATIVE_WORDS: [&str; 10] = [
    "script", "hand", "brush", "symbol", "emoji", "dingbat", "wingding", "webding", "ornament",
    "icon",
];

fn decorative(family: &str) -> bool {
    let lower = family.to_lowercase();
    DECORATIVE_WORDS.iter().any(|word| lower.contains(word))
}

pub(super) fn system_families(cx: &App) -> Vec<(String, FamilyKind)> {
    let text = cx.text_system();
    let mut families = text.all_font_names();
    families.sort();
    families.dedup();
    families
        .into_iter()
        .map(|family| {
            if decorative(&family) {
                return (family, FamilyKind::Decorative);
            }
            let id = text.resolve_font(&font(family.clone()));
            let width = |glyph: char| {
                text.advance(id, px(PROBE_PX), glyph)
                    .map_or(0.0, |size| size.width.as_f32())
            };
            let monospace = (width(NARROW) - width(WIDE)).abs() < WIDTH_TOLERANCE;
            let kind = if monospace {
                FamilyKind::Monospace
            } else {
                FamilyKind::Text
            };
            (family, kind)
        })
        .collect()
}
