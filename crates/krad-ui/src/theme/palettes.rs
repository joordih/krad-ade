use krad_session::AnsiPalette;

use super::{Palette, ShadowLayer, Shadows, SyntaxColors, Tint};

const LIGHT_INK: u32 = 0x0026_232b;
const DARK_INK: u32 = 0x00ed_eaf0;
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

pub(super) const fn light_shadows(ink: u32) -> Shadows {
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

pub(super) const fn dark_shadows(ink: u32) -> Shadows {
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
    backdrop_from: 0x00ea_dcf5,
    backdrop_to: 0x00f6_e9da,
    text_strong: LIGHT_INK,
    text_default: 0x005e_5a61,
    text_subtle: 0x0065_5f6c,
    text_muted: 0x007d_7886,
    text_disabled: 0x00a3_9eac,
    bg_surface: 0x00fd_fafc,
    bg_row_selected: 0x00ff_ffff,
    bg_panel: 0x00f7_f3f6,
    bg_hover: Tint::new(LIGHT_INK, WASH_HOVER),
    bg_selected: Tint::new(LIGHT_INK, WASH_SELECTED),
    border: Tint::new(LIGHT_INK, WASH_BORDER),
    divider: Tint::new(LIGHT_INK, WASH_BORDER),
    scrim: Tint::new(LIGHT_INK, LIGHT_SCRIM),
    attention: 0x00a8_6d0c,
    attention_text: 0x008f_5c08,
    chip_bg: 0x00ec_e6ed,
    chip_fg: 0x0065_5f6c,
    accent: 0x0000_77e6,
    accent_hover: 0x0000_6bcf,
    accent_pressed: 0x0000_5fb8,
    on_accent: 0x00ff_ffff,
    success_fg: 0x0015_b042,
    warning_fg: 0x00b7_791f,
    error_fg: 0x00e5_484d,
    info_fg: 0x0000_77e6,
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
    backdrop_from: 0x001d_1824,
    backdrop_to: 0x0020_1b17,
    text_strong: DARK_INK,
    text_default: 0x00b9_b4bd,
    text_subtle: 0x009a_95a1,
    text_muted: 0x008a_8591,
    text_disabled: 0x005e_5a63,
    bg_surface: 0x0019_171b,
    bg_row_selected: 0x002a_272d,
    bg_panel: 0x0020_1e23,
    bg_hover: Tint::new(DARK_INK, WASH_HOVER),
    bg_selected: Tint::new(DARK_INK, WASH_SELECTED),
    border: Tint::new(DARK_INK, WASH_BORDER),
    divider: Tint::new(DARK_INK, WASH_BORDER),
    scrim: Tint::new(SHADOW_BLACK, DARK_SCRIM),
    attention: 0x00e0_a83a,
    attention_text: 0x00e0_a83a,
    chip_bg: 0x002a_272d,
    chip_fg: 0x00a6_a1ab,
    accent: 0x003b_97f0,
    accent_hover: 0x0052_a4f2,
    accent_pressed: 0x002f_86db,
    on_accent: 0x00ff_ffff,
    success_fg: 0x003d_d068,
    warning_fg: 0x00e0_a83a,
    error_fg: 0x00f2_666b,
    info_fg: 0x003b_97f0,
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

    fn catalog_palettes() -> Vec<&'static Palette> {
        super::super::catalog::app_palettes()
            .into_iter()
            .flat_map(|palettes| {
                [
                    palettes.of(crate::theme::Appearance::Light),
                    palettes.of(crate::theme::Appearance::Dark),
                ]
            })
            .collect()
    }

    #[test]
    fn body_text_is_readable_on_cards() {
        for pal in [&LIGHT, &DARK].into_iter().chain(catalog_palettes()) {
            assert!(contrast(pal.text_strong, pal.bg_surface) >= MIN_TEXT_CONTRAST);
            assert!(contrast(pal.text_default, pal.bg_surface) >= MIN_TEXT_CONTRAST);
        }
    }

    fn css_layer(layer: &ShadowLayer) -> String {
        let [r, g, b] = [16, 8, 0].map(|shift| (layer.tint.color >> shift) & 0xff);
        let spread = if layer.spread == NO_SPREAD {
            String::new()
        } else {
            format!(" {}px", layer.spread)
        };
        format!(
            "0px {}px {}px{spread} rgba({r}, {g}, {b}, {})",
            layer.y, layer.blur, layer.tint.alpha
        )
    }

    fn css_shadow(layers: &[ShadowLayer]) -> String {
        layers.iter().map(css_layer).collect::<Vec<_>>().join(", ")
    }

    #[test]
    fn catalog_shadows_match_the_theme_file() {
        let json = include_str!("../../../../assets/themes/themes.json");
        let declared = |name: &str| -> Vec<String> {
            let key = format!("\"{name}\": \"");
            json.match_indices(&key)
                .map(|(at, _)| {
                    let value = &json[at + key.len()..];
                    value[..value.find('"').unwrap_or(0)].to_owned()
                })
                .collect()
        };
        let palettes = catalog_palettes();
        for (name, shadow) in [
            (
                "shadow-card",
                (|pal: &Palette| css_shadow(&pal.shadows.card)) as fn(&Palette) -> String,
            ),
            ("shadow-row", |pal| css_shadow(&pal.shadows.row)),
            ("shadow-floating", |pal| css_shadow(&[pal.shadows.floating])),
        ] {
            let derived: Vec<String> = palettes.iter().map(|pal| shadow(pal)).collect();
            assert_eq!(declared(name), derived, "{name}");
        }
    }
}
