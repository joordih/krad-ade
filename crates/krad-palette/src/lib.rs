mod builtin;
mod color_math;
mod css;
mod hue;
mod library;
mod palette;
pub mod tokens;
mod tuning;

pub use builtin::{themed, DARK, LIGHT};
pub use color_math::hsl_rgb;
pub use css::{settings_css, settled_palette, tokens_css};
pub use hue::hue_rgb;
pub use library::{krad_palette, krad_theme, theme_library};
pub use palette::{Palette, ShadowLayer, Shadows, SyntaxColors, Tint, ANSI_COLORS};
pub use tuning::{theme_diff, tune, PaletteTuning};

pub const XTERM_ANSI: [u32; ANSI_COLORS] = [
    0x00_00_00, 0xcd_00_00, 0x00_cd_00, 0xcd_cd_00, 0x00_00_ee, 0xcd_00_cd, 0x00_cd_cd, 0xe5_e5_e5,
    0x7f_7f_7f, 0xff_00_00, 0x00_ff_00, 0xff_ff_00, 0x5c_5c_ff, 0xff_00_ff, 0x00_ff_ff, 0xff_ff_ff,
];
