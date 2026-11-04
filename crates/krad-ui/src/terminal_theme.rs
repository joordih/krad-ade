use gpui_kit::App;
use krad_session::AnsiPalette;

use crate::theme::current_theme;

const OPAQUE: u32 = 0xff;

pub(crate) struct TerminalColors {
    pub(crate) background: u32,
    pub(crate) foreground: u32,
    pub(crate) cursor: u32,
    pub(crate) selection: (u32, u32),
    pub(crate) ansi: AnsiPalette,
}

impl From<&krad_domain::TerminalColors> for TerminalColors {
    fn from(colors: &krad_domain::TerminalColors) -> Self {
        Self {
            background: colors.background.0,
            foreground: colors.foreground.0,
            cursor: colors.cursor.0,
            selection: (colors.selection.0, OPAQUE),
            ansi: AnsiPalette(colors.ansi.map(|color| color.0)),
        }
    }
}

pub(crate) fn terminal_colors(cx: &App) -> TerminalColors {
    TerminalColors::from(&current_theme(cx).terminal)
}
