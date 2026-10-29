use std::borrow::Cow;
use std::path::Path;

pub(super) const UI_FONT_FAMILY: &str = ".SystemUIFont";
pub(super) const MONO_FONT_FAMILY: &str = "SF Mono";
pub(super) const SYSTEM_MONO_LABEL: &str = MONO_FONT_FAMILY;

const SF_MONO_DIR: &str = "/System/Applications/Utilities/Terminal.app/Contents/Resources/Fonts";
const SF_MONO_FILES: [&str; 5] = [
    "SF-Mono-Regular.otf",
    "SF-Mono-RegularItalic.otf",
    "SF-Mono-Medium.otf",
    "SF-Mono-Bold.otf",
    "SF-Mono-BoldItalic.otf",
];

pub(super) fn platform_fonts() -> Vec<Cow<'static, [u8]>> {
    let dir = Path::new(SF_MONO_DIR);
    SF_MONO_FILES
        .iter()
        .filter_map(|file| std::fs::read(dir.join(file)).ok())
        .map(Cow::Owned)
        .collect()
}
