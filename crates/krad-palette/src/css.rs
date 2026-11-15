use krad_domain::{AppearanceMode, Settings, Theme, ThemeAppearance, ThemeTuning};
use std::fmt::Write as _;

use crate::builtin::themed;
use crate::library::{krad_palette, krad_theme};
use crate::palette::{Palette, ShadowLayer, Tint};
use crate::tokens;
use crate::tuning::{tune, PaletteTuning};

const PREFIX: &str = "--krad-";
const CHANNEL_SHIFTS: [u32; 3] = [16, 8, 0];
const CHANNEL_MASK: u32 = 0xff;
const DARK_QUERY: &str = "@media (prefers-color-scheme: dark)";

const TOKENS: [(&str, f32); 38] = [
    ("text-2xs", tokens::TEXT_2XS),
    ("line-2xs", tokens::LINE_2XS),
    ("text-xs", tokens::TEXT_XS),
    ("line-xs", tokens::LINE_XS),
    ("text-sm", tokens::TEXT_SM),
    ("line-sm", tokens::LINE_SM),
    ("text-base", tokens::TEXT_BASE),
    ("line-base", tokens::LINE_BASE),
    ("text-lg", tokens::TEXT_LG),
    ("line-lg", tokens::LINE_LG),
    ("icon-xs", tokens::ICON_XS),
    ("icon-sm", tokens::ICON_SM),
    ("icon", tokens::ICON),
    ("icon-lg", tokens::ICON_LG),
    ("radius-xs", tokens::RADIUS_XS),
    ("radius-sm", tokens::RADIUS_SM),
    ("radius-md", tokens::RADIUS_MD),
    ("radius-lg", tokens::RADIUS_LG),
    ("radius-xl", tokens::RADIUS_XL),
    ("button-h", tokens::BUTTON_H),
    ("cta-sm-h", tokens::CTA_SM_H),
    ("icon-button", tokens::ICON_BUTTON),
    ("icon-button-sm", tokens::ICON_BUTTON_SM),
    ("nav-row-h", tokens::NAV_ROW_H),
    ("toolbar-h", tokens::TOOLBAR_H),
    ("input-h", tokens::INPUT_H),
    ("space-0-5", tokens::SPACE_0_5),
    ("space-1", tokens::SPACE_1),
    ("space-1-5", tokens::SPACE_1_5),
    ("space-2", tokens::SPACE_2),
    ("space-3", tokens::SPACE_3),
    ("space-5", tokens::SPACE_5),
    ("space-6", tokens::SPACE_6),
    ("window-inset", tokens::WINDOW_INSET),
    ("card-header-h", tokens::CARD_HEADER_H),
    ("view-header-h", tokens::VIEW_HEADER_H),
    ("chip-h", tokens::CHIP_H),
    ("tab-strip-h", tokens::TAB_STRIP_H),
];

fn channels(color: u32) -> [u32; 3] {
    CHANNEL_SHIFTS.map(|shift| (color >> shift) & CHANNEL_MASK)
}

fn hex(color: u32) -> String {
    let [red, green, blue] = channels(color);
    format!("#{red:02x}{green:02x}{blue:02x}")
}

fn wash(tint: Tint) -> String {
    let [red, green, blue] = channels(tint.color);
    format!("rgb({red} {green} {blue} / {})", tint.alpha)
}

fn shadow(layer: &ShadowLayer) -> String {
    format!(
        "0 {}px {}px {}px {}",
        layer.y,
        layer.blur,
        layer.spread,
        wash(layer.tint)
    )
}

fn shadows(layers: &[ShadowLayer]) -> String {
    layers.iter().map(shadow).collect::<Vec<_>>().join(", ")
}

fn colors(palette: &Palette) -> Vec<(String, String)> {
    let solid = [
        ("backdrop-from", palette.backdrop_from),
        ("backdrop-to", palette.backdrop_to),
        ("shell-ink", palette.shell_ink),
        ("text-strong", palette.text_strong),
        ("text-default", palette.text_default),
        ("text-subtle", palette.text_subtle),
        ("text-muted", palette.text_muted),
        ("text-disabled", palette.text_disabled),
        ("bg-surface", palette.bg_surface),
        ("bg-row-selected", palette.bg_row_selected),
        ("bg-panel", palette.bg_panel),
        ("attention", palette.attention),
        ("attention-text", palette.attention_text),
        ("chip-bg", palette.chip_bg),
        ("chip-fg", palette.chip_fg),
        ("accent", palette.accent),
        ("accent-hover", palette.accent_hover),
        ("accent-pressed", palette.accent_pressed),
        ("on-accent", palette.on_accent),
        ("success", palette.success_fg),
        ("warning", palette.warning_fg),
        ("error", palette.error_fg),
        ("info", palette.info_fg),
        ("diff-added", palette.diff_added),
        ("diff-removed", palette.diff_removed),
        ("diff-modified", palette.diff_modified),
        ("syntax-keyword", palette.syntax.keyword),
        ("syntax-string", palette.syntax.string),
        ("syntax-comment", palette.syntax.comment),
        ("syntax-number", palette.syntax.number),
        ("syntax-type", palette.syntax.type_name),
    ];
    let washes = [
        ("bg-hover", palette.bg_hover),
        ("bg-selected", palette.bg_selected),
        ("border", palette.border),
        ("divider", palette.divider),
        ("scrim", palette.scrim),
    ];
    solid
        .into_iter()
        .map(|(name, color)| (name.to_owned(), hex(color)))
        .chain(
            washes
                .into_iter()
                .map(|(name, tint)| (name.to_owned(), wash(tint))),
        )
        .chain(
            palette
                .ansi
                .iter()
                .enumerate()
                .map(|(index, color)| (format!("ansi-{index}"), hex(*color))),
        )
        .chain([
            ("shadow-card".to_owned(), shadows(&palette.shadows.card)),
            ("shadow-row".to_owned(), shadows(&palette.shadows.row)),
            (
                "shadow-floating".to_owned(),
                shadow(&palette.shadows.floating),
            ),
        ])
        .collect()
}

fn declarations(out: &mut String, palette: &Palette, scheme: &str) {
    let _ = writeln!(out, "  color-scheme: {scheme};");
    for (name, value) in colors(palette) {
        let _ = writeln!(out, "  {PREFIX}{name}: {value};");
    }
}

#[must_use]
pub fn settled_palette(theme: &Theme, tuning: &ThemeTuning) -> Palette {
    let base = krad_palette(&theme.id)
        .cloned()
        .unwrap_or_else(|| themed(theme));
    let tuning = PaletteTuning::from(tuning);
    if tuning.is_neutral() {
        return base;
    }
    tune(&base, tuning, theme.appearance)
}

fn slot_palette(settings: &Settings, library: &[Theme], slot: ThemeAppearance) -> Palette {
    let selection = settings.theme_selection(None);
    let wanted = selection.for_appearance(slot);
    let theme = library
        .iter()
        .find(|theme| &theme.id == wanted)
        .cloned()
        .unwrap_or_else(|| krad_theme(slot));
    settled_palette(&theme, &settings.theme_tuning)
}

#[must_use]
pub fn tokens_css(scale: f32) -> String {
    let mut out = String::from(":root {\n");
    let _ = writeln!(out, "  {PREFIX}scale: {scale};");
    for (name, value) in TOKENS {
        let _ = writeln!(out, "  {PREFIX}{name}: {value}px;");
    }
    out.push_str("}\n");
    out
}

#[must_use]
pub fn settings_css(settings: &Settings, library: &[Theme]) -> String {
    let light = slot_palette(settings, library, ThemeAppearance::Light);
    let dark = slot_palette(settings, library, ThemeAppearance::Dark);
    let mut out = tokens_css(settings.theme_tuning.interface_scale.factor());
    out.push_str(":root {\n");
    match settings.appearance {
        AppearanceMode::Dark => declarations(&mut out, &dark, "dark"),
        AppearanceMode::Light | AppearanceMode::System => {
            declarations(&mut out, &light, "light");
        }
    }
    out.push_str("}\n");
    if settings.appearance == AppearanceMode::System {
        let _ = writeln!(out, "{DARK_QUERY} {{\n:root {{");
        declarations(&mut out, &dark, "dark");
        out.push_str("}\n}\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DARK, LIGHT};

    #[test]
    fn a_palette_becomes_hex_colours_washes_and_shadows() {
        let declared: std::collections::HashMap<String, String> =
            colors(&LIGHT).into_iter().collect();
        assert_eq!(declared["text-strong"], "#1d1d1f");
        assert_eq!(declared["accent"], "#007aff");
        assert_eq!(declared["bg-hover"], "rgb(29 29 31 / 0.05)");
        assert_eq!(declared["ansi-0"], "#1d1d1f");
        assert!(declared["shadow-card"].starts_with("0 0px 0px 1px rgb(29 29 31"));
    }

    #[test]
    fn the_system_mode_answers_both_appearances_and_a_fixed_mode_one() {
        let library = crate::theme_library(Vec::new());
        let mut settings = Settings {
            appearance: AppearanceMode::System,
            ..Settings::default()
        };
        let both = settings_css(&settings, &library);
        assert!(both.contains(DARK_QUERY));
        assert!(both.contains(&hex(LIGHT.accent)));
        assert!(both.contains(&hex(DARK.accent)));
        settings.appearance = AppearanceMode::Dark;
        let dark = settings_css(&settings, &library);
        assert!(!dark.contains(DARK_QUERY));
        assert!(dark.contains("color-scheme: dark"));
        assert!(dark.contains(&format!("{PREFIX}text-sm: 13px;")));
    }
}
