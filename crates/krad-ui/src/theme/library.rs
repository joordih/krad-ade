use gpui_kit::{App, Global};
use krad_domain::{
    InterfaceColors, SyntaxColors, TerminalColors, Theme, ThemeAppearance, ThemeColor, ThemeId,
    KRAD_DARK_THEME, KRAD_LIGHT_THEME,
};

use super::palettes::{DARK, LIGHT};
use super::Palette;

const KRAD_LIGHT_NAME: &str = "Krad Light";
const KRAD_DARK_NAME: &str = "Krad Dark";
const SELECTION_MIX: f32 = 0.7;
const COMMENT: usize = 8;
const RED: usize = 1;
const GREEN: usize = 2;
const YELLOW: usize = 3;
const BLUE: usize = 4;
const MAGENTA: usize = 5;
const CYAN: usize = 6;
const BRIGHT_BLUE: usize = 12;

const SHIPPED_THEMES: [&str; 25] = [
    include_str!("../../../../assets/themes/builtin/xcode-light.json"),
    include_str!("../../../../assets/themes/builtin/xcode-dark.json"),
    include_str!("../../../../assets/themes/builtin/krad-paper.json"),
    include_str!("../../../../assets/themes/builtin/krad-dusk.json"),
    include_str!("../../../../assets/themes/builtin/krad-forest.json"),
    include_str!("../../../../assets/themes/library/ayu-dark.json"),
    include_str!("../../../../assets/themes/library/ayu-light.json"),
    include_str!("../../../../assets/themes/library/ayu-mirage.json"),
    include_str!("../../../../assets/themes/library/catppuccin-frappe.json"),
    include_str!("../../../../assets/themes/library/catppuccin-latte.json"),
    include_str!("../../../../assets/themes/library/catppuccin-macchiato.json"),
    include_str!("../../../../assets/themes/library/catppuccin-mocha.json"),
    include_str!("../../../../assets/themes/library/dracula.json"),
    include_str!("../../../../assets/themes/library/github-dark-default.json"),
    include_str!("../../../../assets/themes/library/github-light-default.json"),
    include_str!("../../../../assets/themes/library/gruvbox-dark-medium.json"),
    include_str!("../../../../assets/themes/library/gruvbox-light-medium.json"),
    include_str!("../../../../assets/themes/library/night-owl-light.json"),
    include_str!("../../../../assets/themes/library/night-owl.json"),
    include_str!("../../../../assets/themes/library/nord.json"),
    include_str!("../../../../assets/themes/library/one-dark-pro.json"),
    include_str!("../../../../assets/themes/library/solarized-dark.json"),
    include_str!("../../../../assets/themes/library/solarized-light.json"),
    include_str!("../../../../assets/themes/library/tokyo-night-light.json"),
    include_str!("../../../../assets/themes/library/tokyo-night.json"),
];

#[derive(Clone, Debug, Default)]
pub(crate) struct ThemeLibrary {
    pub(crate) themes: Vec<Theme>,
}

impl Global for ThemeLibrary {}

pub(super) fn krad_theme(palette: &Palette, appearance: ThemeAppearance) -> Theme {
    let color = ThemeColor;
    let ansi = palette.ansi.0.map(ThemeColor);
    let foreground = color(palette.text_strong);
    let surface = color(palette.bg_surface);
    let (id, name) = match appearance {
        ThemeAppearance::Light => (KRAD_LIGHT_THEME, KRAD_LIGHT_NAME),
        ThemeAppearance::Dark => (KRAD_DARK_THEME, KRAD_DARK_NAME),
    };
    Theme {
        id: ThemeId(id.to_owned()),
        name: name.to_owned(),
        appearance,
        credit: None,
        interface: InterfaceColors {
            background: color(palette.backdrop_from),
            surface,
            panel: color(palette.bg_panel),
            text: foreground,
            muted_text: color(palette.text_muted),
            accent: color(palette.accent),
            on_accent: color(palette.on_accent),
            success: color(palette.success_fg),
            warning: color(palette.warning_fg),
            error: color(palette.error_fg),
            info: color(palette.info_fg),
        },
        terminal: TerminalColors {
            background: surface,
            foreground,
            cursor: color(palette.accent),
            selection: color(palette.accent).mix(surface, SELECTION_MIX),
            ansi,
        },
        syntax: SyntaxColors {
            comment: ansi[COMMENT],
            keyword: ansi[MAGENTA],
            string: ansi[GREEN],
            escape: ansi[CYAN],
            number: ansi[YELLOW],
            constant: ansi[YELLOW],
            attribute: ansi[YELLOW],
            type_name: ansi[CYAN],
            operator: ansi[CYAN],
            function: ansi[BLUE],
            property: ansi[BRIGHT_BLUE],
            tag: ansi[RED],
            variable: foreground,
            punctuation: foreground,
        },
    }
}

fn shipped_themes() -> impl Iterator<Item = Theme> {
    SHIPPED_THEMES.iter().filter_map(|json| {
        json.parse::<Theme>()
            .inspect_err(|error| tracing::warn!(%error, "a shipped theme could not be read"))
            .ok()
    })
}

pub(crate) fn is_krad_theme(id: &ThemeId) -> Option<&'static Palette> {
    match id.0.as_str() {
        KRAD_LIGHT_THEME => Some(&LIGHT),
        KRAD_DARK_THEME => Some(&DARK),
        _ => None,
    }
}

pub fn install_theme_library(user: Vec<Theme>, cx: &mut App) {
    let themes = [
        krad_theme(&LIGHT, ThemeAppearance::Light),
        krad_theme(&DARK, ThemeAppearance::Dark),
    ]
    .into_iter()
    .chain(shipped_themes())
    .chain(user)
    .collect();
    cx.set_global(ThemeLibrary { themes });
}

pub(crate) fn theme_library(cx: &App) -> &[Theme] {
    cx.try_global::<ThemeLibrary>()
        .map_or(&[][..], |library| library.themes.as_slice())
}

pub(crate) fn add_to_library(theme: Theme, cx: &mut App) {
    let library = cx.default_global::<ThemeLibrary>();
    library.themes.retain(|known| known.id != theme.id);
    library.themes.push(theme);
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_THEME_PREFIXES: [&str; 2] = ["xcode", "krad"];

    #[test]
    fn every_shipped_theme_reads_and_has_its_own_id() {
        let themes: Vec<Theme> = shipped_themes().collect();
        assert_eq!(themes.len(), SHIPPED_THEMES.len());
        let mut ids: Vec<&str> = themes.iter().map(|theme| theme.id.0.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), themes.len());
        assert!(themes
            .iter()
            .filter(|theme| !OWN_THEME_PREFIXES
                .iter()
                .any(|prefix| theme.id.0.starts_with(prefix)))
            .all(|theme| theme.credit.is_some()));
    }
}
