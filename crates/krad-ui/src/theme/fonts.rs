#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(target_os = "macos"))]
use other as platform;

use gpui_kit::{FontFeatures, SharedString};
use krad_domain::{FontChoice, FontSize, Ligatures};
use std::borrow::Cow;
use std::sync::Arc;

pub(crate) const UI_FONT_FAMILY: &str = platform::UI_FONT_FAMILY;
pub(super) const MONO_FONT_FAMILY: &str = platform::MONO_FONT_FAMILY;
pub(crate) const SYSTEM_MONO_LABEL: &str = platform::SYSTEM_MONO_LABEL;
const JETBRAINS_MONO_FAMILY: &str = "JetBrains Mono";
pub(crate) const BUNDLED_MONO_FAMILIES: [&str; 2] = [JETBRAINS_MONO_FAMILY, MONO_FONT_FAMILY];
pub(crate) const TERMINAL_FONT_SIZE: FontSize = FontSize::new(12);
pub(crate) const EDITOR_FONT_SIZE: FontSize = FontSize::new(14);
const LIGATURE_FEATURES: [&str; 2] = ["calt", "liga"];
const TABULAR_DIGITS: &str = "tnum";
const FEATURE_ON: u32 = 1;
const FEATURE_OFF: u32 = 0;

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

pub(crate) fn mono_family(choice: &FontChoice) -> SharedString {
    SharedString::from(choice.family_or(MONO_FONT_FAMILY).to_owned())
}

pub(crate) fn interface_family(choice: &FontChoice) -> SharedString {
    SharedString::from(choice.family_or(UI_FONT_FAMILY).to_owned())
}

pub(crate) fn tabular_digits() -> FontFeatures {
    FontFeatures(Arc::new(vec![(TABULAR_DIGITS.to_owned(), FEATURE_ON)]))
}

pub(crate) fn font_features(ligatures: Ligatures) -> FontFeatures {
    let value = match ligatures {
        Ligatures::On => FEATURE_ON,
        Ligatures::Off => FEATURE_OFF,
    };
    FontFeatures(Arc::new(
        LIGATURE_FEATURES
            .iter()
            .map(|feature| ((*feature).to_owned(), value))
            .collect(),
    ))
}
