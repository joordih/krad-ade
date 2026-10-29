#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(target_os = "macos"))]
use other as platform;

use std::borrow::Cow;

pub(crate) const UI_FONT_FAMILY: &str = platform::UI_FONT_FAMILY;
pub(super) const MONO_FONT_FAMILY: &str = platform::MONO_FONT_FAMILY;
pub(crate) const SYSTEM_MONO_LABEL: &str = platform::SYSTEM_MONO_LABEL;
pub(super) const JETBRAINS_MONO_FAMILY: &str = "JetBrains Mono";

const JETBRAINS_MONO: [&[u8]; 4] = [
    include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../../../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../../../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
];

pub(super) fn app_fonts() -> Vec<Cow<'static, [u8]>> {
    JETBRAINS_MONO
        .iter()
        .map(|bytes| Cow::Borrowed(*bytes))
        .chain(platform::platform_fonts())
        .collect()
}
