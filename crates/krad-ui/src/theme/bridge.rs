use krad_session::AnsiPalette;

use super::{Palette, ShadowLayer, Shadows, SyntaxColors, Tint};

impl From<krad_palette::Tint> for Tint {
    fn from(tint: krad_palette::Tint) -> Self {
        Self::new(tint.color, tint.alpha)
    }
}

impl From<Tint> for krad_palette::Tint {
    fn from(tint: Tint) -> Self {
        Self::new(tint.color, tint.alpha)
    }
}

impl From<krad_palette::ShadowLayer> for ShadowLayer {
    fn from(layer: krad_palette::ShadowLayer) -> Self {
        Self {
            y: layer.y,
            blur: layer.blur,
            spread: layer.spread,
            tint: layer.tint.into(),
        }
    }
}

impl From<ShadowLayer> for krad_palette::ShadowLayer {
    fn from(layer: ShadowLayer) -> Self {
        Self {
            y: layer.y,
            blur: layer.blur,
            spread: layer.spread,
            tint: layer.tint.into(),
        }
    }
}

impl From<krad_palette::Shadows> for Shadows {
    fn from(shadows: krad_palette::Shadows) -> Self {
        Self {
            card: shadows.card.map(ShadowLayer::from),
            row: shadows.row.map(ShadowLayer::from),
            floating: shadows.floating.into(),
        }
    }
}

impl From<Shadows> for krad_palette::Shadows {
    fn from(shadows: Shadows) -> Self {
        Self {
            card: shadows.card.map(krad_palette::ShadowLayer::from),
            row: shadows.row.map(krad_palette::ShadowLayer::from),
            floating: shadows.floating.into(),
        }
    }
}

impl From<&krad_palette::SyntaxColors> for SyntaxColors {
    fn from(syntax: &krad_palette::SyntaxColors) -> Self {
        Self {
            keyword: syntax.keyword,
            string: syntax.string,
            comment: syntax.comment,
            number: syntax.number,
            type_name: syntax.type_name,
        }
    }
}

impl From<&SyntaxColors> for krad_palette::SyntaxColors {
    fn from(syntax: &SyntaxColors) -> Self {
        Self {
            keyword: syntax.keyword,
            string: syntax.string,
            comment: syntax.comment,
            number: syntax.number,
            type_name: syntax.type_name,
        }
    }
}

impl From<&krad_palette::Palette> for Palette {
    fn from(palette: &krad_palette::Palette) -> Self {
        Self {
            backdrop_from: palette.backdrop_from,
            backdrop_to: palette.backdrop_to,
            shell_ink: palette.shell_ink,
            text_strong: palette.text_strong,
            text_default: palette.text_default,
            text_subtle: palette.text_subtle,
            text_muted: palette.text_muted,
            text_disabled: palette.text_disabled,
            bg_surface: palette.bg_surface,
            bg_row_selected: palette.bg_row_selected,
            bg_panel: palette.bg_panel,
            bg_hover: palette.bg_hover.into(),
            bg_selected: palette.bg_selected.into(),
            border: palette.border.into(),
            divider: palette.divider.into(),
            scrim: palette.scrim.into(),
            attention: palette.attention,
            attention_text: palette.attention_text,
            chip_bg: palette.chip_bg,
            chip_fg: palette.chip_fg,
            accent: palette.accent,
            accent_hover: palette.accent_hover,
            accent_pressed: palette.accent_pressed,
            on_accent: palette.on_accent,
            success_fg: palette.success_fg,
            warning_fg: palette.warning_fg,
            error_fg: palette.error_fg,
            info_fg: palette.info_fg,
            diff_added: palette.diff_added,
            diff_removed: palette.diff_removed,
            diff_modified: palette.diff_modified,
            ansi: AnsiPalette(palette.ansi),
            syntax: SyntaxColors::from(&palette.syntax),
            shadows: palette.shadows.into(),
        }
    }
}

impl From<&Palette> for krad_palette::Palette {
    fn from(palette: &Palette) -> Self {
        Self {
            backdrop_from: palette.backdrop_from,
            backdrop_to: palette.backdrop_to,
            shell_ink: palette.shell_ink,
            text_strong: palette.text_strong,
            text_default: palette.text_default,
            text_subtle: palette.text_subtle,
            text_muted: palette.text_muted,
            text_disabled: palette.text_disabled,
            bg_surface: palette.bg_surface,
            bg_row_selected: palette.bg_row_selected,
            bg_panel: palette.bg_panel,
            bg_hover: palette.bg_hover.into(),
            bg_selected: palette.bg_selected.into(),
            border: palette.border.into(),
            divider: palette.divider.into(),
            scrim: palette.scrim.into(),
            attention: palette.attention,
            attention_text: palette.attention_text,
            chip_bg: palette.chip_bg,
            chip_fg: palette.chip_fg,
            accent: palette.accent,
            accent_hover: palette.accent_hover,
            accent_pressed: palette.accent_pressed,
            on_accent: palette.on_accent,
            success_fg: palette.success_fg,
            warning_fg: palette.warning_fg,
            error_fg: palette.error_fg,
            info_fg: palette.info_fg,
            diff_added: palette.diff_added,
            diff_removed: palette.diff_removed,
            diff_modified: palette.diff_modified,
            ansi: palette.ansi.0,
            syntax: krad_palette::SyntaxColors::from(&palette.syntax),
            shadows: palette.shadows.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_palette_survives_the_trip_through_krad_palette() {
        for source in [&krad_palette::LIGHT, &krad_palette::DARK] {
            let back = krad_palette::Palette::from(&Palette::from(source));
            assert_eq!(&back, source);
        }
    }
}
