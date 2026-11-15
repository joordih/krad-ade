use krad_domain::Theme;
use std::sync::LazyLock;

use super::Palette;

pub(super) static LIGHT: LazyLock<Palette> = LazyLock::new(|| Palette::from(&krad_palette::LIGHT));
pub(super) static DARK: LazyLock<Palette> = LazyLock::new(|| Palette::from(&krad_palette::DARK));

pub(super) fn themed(theme: &Theme) -> Palette {
    Palette::from(&krad_palette::themed(theme))
}
