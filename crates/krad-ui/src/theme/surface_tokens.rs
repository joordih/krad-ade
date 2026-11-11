use gpui_kit::{px, App, BoxShadow, Rgba};

use super::{appearance, Appearance, Palette, Tint, SPACE_1_5};

pub(crate) const FLOATING_RADIUS: f32 = 12.0;
pub(crate) const FLOATING_PAD: f32 = SPACE_1_5;
pub(crate) const FLOATING_ROW_RADIUS: f32 = FLOATING_RADIUS - FLOATING_PAD;
pub(crate) const FLOATING_GAP: f32 = 4.0;
pub(crate) const FLOATING_EDGE_MARGIN: f32 = 8.0;
pub(crate) const FLOATING_BORDER_ALPHA: f32 = 0.10;

pub(crate) const MENU_W: f32 = 236.0;
pub(crate) const MENU_MIN_W: f32 = 160.0;
pub(crate) const MENU_WIDE_W: f32 = 304.0;
pub(crate) const MENU_ROW_H: f32 = 30.0;
pub(crate) const MENU_ROW_DETAIL_H: f32 = 44.0;
pub(crate) const MENU_HEADER_H: f32 = 24.0;
pub(crate) const MENU_SEPARATOR_H: f32 = 9.0;
pub(crate) const MENU_SEARCH_H: f32 = 40.0;

pub(crate) const DIALOG_RADIUS: f32 = 16.0;
const DIALOG_SCRIM_DARK_ALPHA: f32 = 0.35;
const DIALOG_SCRIM_LIGHT_ALPHA: f32 = 0.18;

const SHADOW_INK: u32 = 0x0000_0000;
const SHADOW_ALPHA: f32 = 0.10;
const FAR_SHADOW: ShadowSpec = ShadowSpec {
    y: 20.0,
    blur: 25.0,
    spread: -5.0,
};
const NEAR_SHADOW: ShadowSpec = ShadowSpec {
    y: 8.0,
    blur: 10.0,
    spread: -6.0,
};

struct ShadowSpec {
    y: f32,
    blur: f32,
    spread: f32,
}

impl From<&ShadowSpec> for BoxShadow {
    fn from(spec: &ShadowSpec) -> Self {
        BoxShadow::new(
            px(0.0),
            px(spec.y),
            Tint::new(SHADOW_INK, SHADOW_ALPHA).into(),
        )
        .blur_radius(px(spec.blur))
        .spread_radius(px(spec.spread))
    }
}

pub(crate) fn floating_shadows() -> Vec<BoxShadow> {
    [&FAR_SHADOW, &NEAR_SHADOW]
        .into_iter()
        .map(BoxShadow::from)
        .collect()
}

pub(crate) fn floating_border(pal: &Palette) -> Rgba {
    Tint::new(pal.text_strong, FLOATING_BORDER_ALPHA).into()
}

pub(crate) fn dialog_scrim(pal: &Palette, cx: &App) -> Rgba {
    let alpha = match appearance(cx) {
        Appearance::Dark => DIALOG_SCRIM_DARK_ALPHA,
        Appearance::Light => DIALOG_SCRIM_LIGHT_ALPHA,
    };
    Tint::new(pal.scrim.color, alpha).into()
}
