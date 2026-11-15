pub const ANSI_COLORS: usize = 16;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tint {
    pub color: u32,
    pub alpha: f32,
}

impl Tint {
    #[must_use]
    pub const fn new(color: u32, alpha: f32) -> Self {
        Self { color, alpha }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShadowLayer {
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub tint: Tint,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shadows {
    pub card: [ShadowLayer; 2],
    pub row: [ShadowLayer; 2],
    pub floating: ShadowLayer,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SyntaxColors {
    pub keyword: u32,
    pub string: u32,
    pub comment: u32,
    pub number: u32,
    pub type_name: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Palette {
    pub backdrop_from: u32,
    pub backdrop_to: u32,
    pub shell_ink: u32,
    pub text_strong: u32,
    pub text_default: u32,
    pub text_subtle: u32,
    pub text_muted: u32,
    pub text_disabled: u32,
    pub bg_surface: u32,
    pub bg_row_selected: u32,
    pub bg_panel: u32,
    pub bg_hover: Tint,
    pub bg_selected: Tint,
    pub border: Tint,
    pub divider: Tint,
    pub scrim: Tint,
    pub attention: u32,
    pub attention_text: u32,
    pub chip_bg: u32,
    pub chip_fg: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_pressed: u32,
    pub on_accent: u32,
    pub success_fg: u32,
    pub warning_fg: u32,
    pub error_fg: u32,
    pub info_fg: u32,
    pub diff_added: u32,
    pub diff_removed: u32,
    pub diff_modified: u32,
    pub ansi: [u32; ANSI_COLORS],
    pub syntax: SyntaxColors,
    pub shadows: Shadows,
}
