use std::borrow::Cow;

pub(super) const UI_FONT_FAMILY: &str = "Inter";
#[cfg(windows)]
pub(super) const MONO_FONT_FAMILY: &str = "Consolas";
#[cfg(not(windows))]
pub(super) const MONO_FONT_FAMILY: &str = "DejaVu Sans Mono";
pub(super) const SYSTEM_MONO_LABEL: &str = "System";

const INTER: [&[u8]; 6] = [
    include_bytes!("../../../../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../../../assets/fonts/Inter-Italic.ttf"),
    include_bytes!("../../../../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../../../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../../../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../../../../assets/fonts/Inter-BoldItalic.ttf"),
];

pub(super) fn platform_fonts() -> Vec<Cow<'static, [u8]>> {
    INTER.iter().map(|bytes| Cow::Borrowed(*bytes)).collect()
}
