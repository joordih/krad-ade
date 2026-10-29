use krad_domain::{AppTheme, AppThemeId, ThemeCatalog, ThemeTokens, Wash};
use krad_session::AnsiPalette;
use std::sync::LazyLock;

use super::palettes::{dark_shadows, light_shadows};
use super::{Appearance, Palette, SyntaxColors, Tint};

const THEMES_JSON: &str = include_str!("../../../../assets/themes/themes.json");

static LIBRARY: LazyLock<Library> = LazyLock::new(|| Library::new(THEMES_JSON));

pub(crate) struct ThemePalettes {
    light: Palette,
    dark: Palette,
}

impl ThemePalettes {
    pub(crate) const fn of(&self, appearance: Appearance) -> &Palette {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }
}

struct Library {
    catalog: ThemeCatalog,
    palettes: Vec<ThemePalettes>,
}

impl Library {
    fn new(json: &str) -> Self {
        let catalog = json.parse().unwrap_or_else(|_| ThemeCatalog {
            default: AppThemeId(String::new()),
            themes: Vec::new(),
        });
        let palettes = catalog
            .themes
            .iter()
            .map(|theme| ThemePalettes {
                light: palette(&theme.light, Appearance::Light),
                dark: palette(&theme.dark, Appearance::Dark),
            })
            .collect();
        Self { catalog, palettes }
    }

    fn get(&self, index: usize) -> Option<(&AppTheme, &ThemePalettes)> {
        Some((self.catalog.themes.get(index)?, self.palettes.get(index)?))
    }
}

pub(super) fn app_palettes() -> Option<&'static ThemePalettes> {
    LIBRARY
        .get(LIBRARY.catalog.resolve(None)?)
        .map(|(_, palettes)| palettes)
}

impl From<Wash> for Tint {
    fn from(wash: Wash) -> Self {
        Self::new(wash.color, wash.alpha)
    }
}

fn palette(tokens: &ThemeTokens, appearance: Appearance) -> Palette {
    let built_in = appearance.palette();
    let ink = tokens.text_strong.0;
    Palette {
        backdrop_from: tokens.backdrop_from.0,
        backdrop_to: tokens.backdrop_to.0,
        text_strong: ink,
        text_default: tokens.text_default.0,
        text_subtle: tokens.text_subtle.0,
        text_muted: tokens.text_muted.0,
        text_disabled: tokens.text_disabled.0,
        bg_surface: tokens.bg_surface.0,
        bg_row_selected: tokens.bg_row_selected.0,
        bg_panel: tokens.bg_panel.0,
        bg_hover: tokens.bg_hover.into(),
        bg_selected: tokens.bg_selected.into(),
        border: tokens.border.into(),
        divider: tokens.divider.into(),
        scrim: built_in.scrim,
        attention: tokens.attention.0,
        attention_text: tokens.attention_text.0,
        chip_bg: tokens.chip_bg.0,
        chip_fg: tokens.chip_fg.0,
        accent: tokens.accent.0,
        accent_hover: tokens.accent_hover.0,
        accent_pressed: tokens.accent_pressed.0,
        on_accent: tokens.on_accent.0,
        success_fg: tokens.success_fg.0,
        warning_fg: tokens.warning_fg.0,
        error_fg: tokens.error_fg.0,
        info_fg: tokens.info_fg.0,
        ansi: AnsiPalette(tokens.ansi()),
        syntax: SyntaxColors {
            keyword: tokens.syntax_keyword.0,
            string: tokens.syntax_string.0,
            comment: tokens.syntax_comment.0,
            number: tokens.syntax_number.0,
            type_name: tokens.syntax_type.0,
        },
        shadows: match appearance {
            Appearance::Light => light_shadows(ink),
            Appearance::Dark => dark_shadows(ink),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_file_holds_the_app_palette() -> Result<(), Box<dyn std::error::Error>> {
        let catalog: ThemeCatalog = THEMES_JSON.parse()?;
        assert_eq!(catalog.themes.len(), 1);
        assert!(app_palettes().is_some());
        Ok(())
    }
}
