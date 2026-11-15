use gpui_kit::{App, Global};
use krad_domain::{Theme, ThemeId, KRAD_DARK_THEME, KRAD_LIGHT_THEME};

use super::palettes::{DARK, LIGHT};
use super::Palette;

#[derive(Clone, Debug, Default)]
pub(crate) struct ThemeLibrary {
    pub(crate) themes: Vec<Theme>,
}

impl Global for ThemeLibrary {}

pub(crate) fn is_krad_theme(id: &ThemeId) -> Option<&'static Palette> {
    match id.0.as_str() {
        KRAD_LIGHT_THEME => Some(&LIGHT),
        KRAD_DARK_THEME => Some(&DARK),
        _ => None,
    }
}

pub fn install_theme_library(user: Vec<Theme>, cx: &mut App) {
    cx.set_global(ThemeLibrary {
        themes: krad_palette::theme_library(user),
    });
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
