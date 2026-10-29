use krad_session::AnsiPalette;

use super::{Palette, ShadowLayer, Shadows, SyntaxColors, Tint};

const LIGHT_INK: u32 = 0x001d_1d1f;
const DARK_INK: u32 = 0x00f5_f5f7;
const SHADOW_BLACK: u32 = 0x0000_0000;

const WASH_HOVER: f32 = 0.05;
const WASH_SELECTED: f32 = 0.08;
const WASH_BORDER: f32 = 0.08;
const LIGHT_SCRIM: f32 = 0.2;
const DARK_SCRIM: f32 = 0.4;

const RING_Y: f32 = 0.0;
const RING_BLUR: f32 = 0.0;
const RING_SPREAD: f32 = 1.0;
const CARD_DROP_Y: f32 = 1.0;
const CARD_DROP_BLUR: f32 = 3.0;
const ROW_DROP_Y: f32 = 1.0;
const ROW_DROP_BLUR: f32 = 2.0;
const FLOATING_Y: f32 = 4.0;
const FLOATING_BLUR: f32 = 16.0;
const NO_SPREAD: f32 = 0.0;

const fn ring(tint: Tint) -> ShadowLayer {
    ShadowLayer {
        y: RING_Y,
        blur: RING_BLUR,
        spread: RING_SPREAD,
        tint,
    }
}

const fn drop(y: f32, blur: f32, tint: Tint) -> ShadowLayer {
    ShadowLayer {
        y,
        blur,
        spread: NO_SPREAD,
        tint,
    }
}

const LIGHT_CARD_RING_ALPHA: f32 = 0.05;
const LIGHT_ROW_RING_ALPHA: f32 = 0.04;
const LIGHT_DROP_ALPHA: f32 = 0.06;
const LIGHT_FLOATING_ALPHA: f32 = 0.1;
const DARK_CARD_RING_ALPHA: f32 = 0.06;
const DARK_ROW_RING_ALPHA: f32 = 0.05;
const DARK_DROP_ALPHA: f32 = 0.4;
const DARK_ROW_DROP_ALPHA: f32 = 0.35;
const DARK_FLOATING_ALPHA: f32 = 0.4;

const fn light_shadows(ink: u32) -> Shadows {
    Shadows {
        card: [
            ring(Tint::new(ink, LIGHT_CARD_RING_ALPHA)),
            drop(
                CARD_DROP_Y,
                CARD_DROP_BLUR,
                Tint::new(ink, LIGHT_DROP_ALPHA),
            ),
        ],
        row: [
            ring(Tint::new(ink, LIGHT_ROW_RING_ALPHA)),
            drop(ROW_DROP_Y, ROW_DROP_BLUR, Tint::new(ink, LIGHT_DROP_ALPHA)),
        ],
        floating: drop(
            FLOATING_Y,
            FLOATING_BLUR,
            Tint::new(ink, LIGHT_FLOATING_ALPHA),
        ),
    }
}

const fn dark_shadows(ink: u32) -> Shadows {
    Shadows {
        card: [
            ring(Tint::new(ink, DARK_CARD_RING_ALPHA)),
            drop(
                CARD_DROP_Y,
                CARD_DROP_BLUR,
                Tint::new(SHADOW_BLACK, DARK_DROP_ALPHA),
            ),
        ],
        row: [
            ring(Tint::new(ink, DARK_ROW_RING_ALPHA)),
            drop(
                ROW_DROP_Y,
                ROW_DROP_BLUR,
                Tint::new(SHADOW_BLACK, DARK_ROW_DROP_ALPHA),
            ),
        ],
        floating: drop(
            FLOATING_Y,
            FLOATING_BLUR,
            Tint::new(SHADOW_BLACK, DARK_FLOATING_ALPHA),
        ),
    }
}

pub(super) const LIGHT: Palette = Palette {
    backdrop_from: 0x00e8_e8ed,
    backdrop_to: 0x00ed_edf0,
    text_strong: LIGHT_INK,
    text_default: 0x0042_4245,
    text_subtle: 0x0058_585d,
    text_muted: 0x006e_6e73,
    text_disabled: 0x00ae_aeb2,
    bg_surface: 0x00ff_ffff,
    bg_row_selected: 0x00ff_ffff,
    bg_panel: 0x00f5_f5f7,
    bg_hover: Tint::new(LIGHT_INK, WASH_HOVER),
    bg_selected: Tint::new(LIGHT_INK, WASH_SELECTED),
    border: Tint::new(LIGHT_INK, WASH_BORDER),
    divider: Tint::new(LIGHT_INK, WASH_BORDER),
    scrim: Tint::new(LIGHT_INK, LIGHT_SCRIM),
    attention: 0x00c2_6a00,
    attention_text: 0x00a3_5a00,
    chip_bg: 0x00e8_e8ed,
    chip_fg: 0x0058_585d,
    accent: 0x0000_7aff,
    accent_hover: 0x0000_71eb,
    accent_pressed: 0x0000_62cc,
    on_accent: 0x00ff_ffff,
    success_fg: 0x0024_8a3d,
    warning_fg: 0x00c2_6a00,
    error_fg: 0x00d7_0015,
    info_fg: 0x0000_7aff,
    ansi: AnsiPalette([
        LIGHT_INK,
        0x00cf_4b4b,
        0x003f_9256,
        0x008a_6d00,
        0x0000_40dd,
        0x008a_63d2,
        0x0052_9aaf,
        0x008e_8e93,
        0x006e_6e73,
        0x00d7_0015,
        0x001f_8a4c,
        0x00a0_7700,
        0x0000_71e3,
        0x00af_52de,
        0x0000_71a4,
        0x003a_3a3c,
    ]),
    syntax: SyntaxColors {
        keyword: 0x009b_2393,
        string: 0x00c4_1a16,
        comment: 0x005d_6c79,
        number: 0x001c_00cf,
        type_name: 0x000b_4f79,
    },
    shadows: light_shadows(LIGHT_INK),
};

pub(super) const DARK: Palette = Palette {
    backdrop_from: 0x001c_1c1e,
    backdrop_to: 0x001f_1f1f,
    text_strong: DARK_INK,
    text_default: 0x00c7_c7cc,
    text_subtle: 0x0098_989d,
    text_muted: 0x008e_8e93,
    text_disabled: 0x005a_5a5e,
    bg_surface: 0x0026_2626,
    bg_row_selected: 0x003a_3a3c,
    bg_panel: 0x002c_2c2e,
    bg_hover: Tint::new(DARK_INK, WASH_HOVER),
    bg_selected: Tint::new(DARK_INK, WASH_SELECTED),
    border: Tint::new(DARK_INK, WASH_BORDER),
    divider: Tint::new(DARK_INK, WASH_BORDER),
    scrim: Tint::new(SHADOW_BLACK, DARK_SCRIM),
    attention: 0x00ff_9f0a,
    attention_text: 0x00ff_9f0a,
    chip_bg: 0x003a_3a3c,
    chip_fg: 0x00ae_aeb2,
    accent: 0x000a_84ff,
    accent_hover: 0x0040_9cff,
    accent_pressed: 0x0000_71e3,
    on_accent: 0x00ff_ffff,
    success_fg: 0x0030_d158,
    warning_fg: 0x00ff_9f0a,
    error_fg: 0x00ff_453a,
    info_fg: 0x000a_84ff,
    ansi: krad_session::XTERM_PALETTE,
    syntax: SyntaxColors {
        keyword: 0x00fc_5fa3,
        string: 0x00fc_6a5d,
        comment: 0x006c_7986,
        number: 0x00d0_bf69,
        type_name: 0x005d_d8ff,
    },
    shadows: dark_shadows(DARK_INK),
};

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_TEXT_CONTRAST: f32 = 4.5;

    fn channels(color: u32) -> [f32; 3] {
        [16, 8, 0].map(|shift| f32::from(u8::try_from((color >> shift) & 0xff).unwrap_or(0)))
    }

    fn luminance(color: u32) -> f32 {
        let linear = channels(color).map(|value| {
            let srgb = value / 255.0;
            if srgb <= 0.039_28 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    fn contrast(a: u32, b: u32) -> f32 {
        let (light, dark) = {
            let (x, y) = (luminance(a), luminance(b));
            (x.max(y), x.min(y))
        };
        (light + 0.05) / (dark + 0.05)
    }

    #[test]
    fn body_text_is_readable_on_cards() {
        for pal in [&LIGHT, &DARK] {
            assert!(contrast(pal.text_strong, pal.bg_surface) >= MIN_TEXT_CONTRAST);
            assert!(contrast(pal.text_default, pal.bg_surface) >= MIN_TEXT_CONTRAST);
        }
    }
}
