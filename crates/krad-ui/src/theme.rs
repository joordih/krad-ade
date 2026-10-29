mod catalog;
mod editor;
mod fonts;
mod palettes;
mod tint;
mod tokens;

use catalog::app_palettes;
pub use editor::apply_code_look;
use fonts::{app_fonts, JETBRAINS_MONO_FAMILY, MONO_FONT_FAMILY};
pub(crate) use fonts::{SYSTEM_MONO_LABEL, UI_FONT_FAMILY};
pub use tint::apply_window_tint;
pub(crate) use tint::hue_rgb;
pub(crate) use tokens::*;

use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::{
    linear_color_stop, linear_gradient, px, rgb, rgba, transparent_black, App, Background,
    BoxShadow, Fill, Global, Hsla, Rgba,
};
use krad_domain::{TerminalFont, WindowMaterial};
use krad_session::AnsiPalette;

use crate::window_frame::{match_window_frame, usable_material};

use self::palettes::{DARK, LIGHT};

const ALPHA_BITS: u32 = 8;
pub(crate) const FOCUS_RING_PX: f32 = 2.0;
const RING_ALPHA: u32 = 0x33;
const BACKDROP_ANGLE_DEG: f32 = 135.0;
const GRADIENT_START: f32 = 0.0;
const GRADIENT_END: f32 = 1.0;
const OPAQUE_ALPHA: u32 = 0xff;
const CLEAR_ALPHA: u32 = 0x00;
const DARK_CHROME_TINT: u32 = 0x0017_1717;
const LIGHT_CHROME_TINT: u32 = 0x00f7_f7f7;
const BLURRED_DARK_ALPHA: u32 = 0xa6;
const BLURRED_LIGHT_ALPHA: u32 = 0xe0;
const MICA_DARK_ALPHA: u32 = CLEAR_ALPHA;
const MICA_LIGHT_ALPHA: u32 = 0xcc;
const GLASS_DARK_ALPHA: u32 = 0xa6;
const GLASS_LIGHT_ALPHA: u32 = 0xcc;
const SURFACE_DARK_ALPHA: u32 = 0x14;
const SURFACE_LIGHT_ALPHA: u32 = 0x40;
const POPOVER_ALPHA: u32 = 0xf2;
const POPOVER_TRANSLUCENT_ALPHA: u32 = 0xc7;
const THEMED_TERMINAL_ALPHA: u32 = 0x1a;
const TRANSLUCENT_SCRIM_ALPHA: f32 = 0.12;
const TRANSLUCENT_ROW_ALPHA: f32 = 0.12;

pub(crate) const fn terminal_font_family(font: TerminalFont) -> &'static str {
    match font {
        TerminalFont::System => MONO_FONT_FAMILY,
        TerminalFont::JetBrainsMono => JETBRAINS_MONO_FAMILY,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

impl Appearance {
    pub(crate) const fn palette(self) -> &'static Palette {
        match self {
            Self::Light => &LIGHT,
            Self::Dark => &DARK,
        }
    }

    const fn kit_mode(self) -> ThemeMode {
        match self {
            Self::Light => ThemeMode::Light,
            Self::Dark => ThemeMode::Dark,
        }
    }
}

struct CurrentTheme {
    appearance: Appearance,
    palette: &'static Palette,
}

impl Global for CurrentTheme {}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Tint {
    color: u32,
    alpha: f32,
}

impl Tint {
    const fn new(color: u32, alpha: f32) -> Self {
        Self { color, alpha }
    }
}

impl From<Tint> for Rgba {
    fn from(tint: Tint) -> Self {
        Self {
            a: tint.alpha,
            ..rgb(tint.color)
        }
    }
}

impl From<Tint> for Hsla {
    fn from(tint: Tint) -> Self {
        Rgba::from(tint).into()
    }
}

impl From<Tint> for Fill {
    fn from(tint: Tint) -> Self {
        Rgba::from(tint).into()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct ShadowLayer {
    y: f32,
    blur: f32,
    spread: f32,
    tint: Tint,
}

impl From<ShadowLayer> for BoxShadow {
    fn from(layer: ShadowLayer) -> Self {
        BoxShadow::new(px(0.0), px(layer.y), layer.tint.into())
            .blur_radius(px(layer.blur))
            .spread_radius(px(layer.spread))
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Shadows {
    card: [ShadowLayer; 2],
    row: [ShadowLayer; 2],
    floating: ShadowLayer,
}

pub(crate) struct Palette {
    pub(crate) backdrop_from: u32,
    pub(crate) backdrop_to: u32,
    pub(crate) text_strong: u32,
    pub(crate) text_default: u32,
    pub(crate) text_subtle: u32,
    pub(crate) text_muted: u32,
    pub(crate) text_disabled: u32,
    pub(crate) bg_surface: u32,
    pub(crate) bg_row_selected: u32,
    pub(crate) bg_panel: u32,
    pub(crate) bg_hover: Tint,
    pub(crate) bg_selected: Tint,
    pub(crate) border: Tint,
    pub(crate) divider: Tint,
    pub(crate) scrim: Tint,
    pub(crate) attention: u32,
    pub(crate) attention_text: u32,
    pub(crate) chip_bg: u32,
    pub(crate) chip_fg: u32,
    pub(crate) accent: u32,
    pub(crate) accent_hover: u32,
    pub(crate) accent_pressed: u32,
    pub(crate) on_accent: u32,
    pub(crate) success_fg: u32,
    pub(crate) warning_fg: u32,
    pub(crate) error_fg: u32,
    pub(crate) info_fg: u32,
    pub(crate) ansi: AnsiPalette,
    pub(crate) syntax: SyntaxColors,
    shadows: Shadows,
}

pub(crate) struct SyntaxColors {
    pub(crate) keyword: u32,
    pub(crate) string: u32,
    pub(crate) comment: u32,
    pub(crate) number: u32,
    pub(crate) type_name: u32,
}

pub(crate) fn palette(cx: &App) -> &'static Palette {
    cx.try_global::<CurrentTheme>()
        .map_or(Appearance::default().palette(), |current| current.palette)
}

pub(crate) fn appearance(cx: &App) -> Appearance {
    cx.try_global::<CurrentTheme>()
        .map_or(Appearance::default(), |current| current.appearance)
}

/// # Errors
pub fn register_fonts(cx: &App) -> gpui_kit::Result<()> {
    cx.text_system().add_fonts(app_fonts())
}

pub fn apply_appearance(appearance: Appearance, cx: &mut App) {
    let palette = app_palettes().map_or(appearance.palette(), |palettes| palettes.of(appearance));
    cx.set_global(CurrentTheme {
        appearance,
        palette,
    });
    restyle_kit(cx);
}

fn restyle_kit(cx: &mut App) {
    let (appearance, palette) = (appearance(cx), palette(cx));
    let material = window_material(cx);
    match_window_frame(appearance == Appearance::Dark);
    Theme::change(appearance.kit_mode(), None, cx);
    Theme::update(cx, |theme| style_kit_theme(theme, palette));
    if material.is_translucent() {
        let popover = popover_fill(palette, cx).into();
        let kit = Theme::global_mut(cx);
        kit.tokens.background = transparent_black().into();
        kit.popover = popover;
    }
    editor::style_editor(cx);
    cx.refresh_windows();
}

pub(crate) fn tinted(color: u32, alpha: u32) -> Rgba {
    rgba((color << ALPHA_BITS) | alpha)
}

pub(crate) fn backdrop(pal: &Palette) -> Background {
    linear_gradient(
        BACKDROP_ANGLE_DEG,
        linear_color_stop(rgb(pal.backdrop_from), GRADIENT_START),
        linear_color_stop(rgb(pal.backdrop_to), GRADIENT_END),
    )
}

struct CurrentMaterial(WindowMaterial);

impl Global for CurrentMaterial {}

pub fn apply_window_material(material: WindowMaterial, cx: &mut App) {
    cx.set_global(CurrentMaterial(usable_material(material)));
    restyle_kit(cx);
}

pub(crate) fn window_material(cx: &App) -> WindowMaterial {
    cx.try_global::<CurrentMaterial>()
        .map_or(WindowMaterial::default(), |current| current.0)
}

const fn chrome_tint(material: WindowMaterial, appearance: Appearance) -> (u32, u32) {
    match (material, appearance) {
        (WindowMaterial::Opaque, _) => (0, OPAQUE_ALPHA),
        (WindowMaterial::Blurred, Appearance::Dark) => (DARK_CHROME_TINT, BLURRED_DARK_ALPHA),
        (WindowMaterial::Blurred, Appearance::Light) => (LIGHT_CHROME_TINT, BLURRED_LIGHT_ALPHA),
        (WindowMaterial::Mica, Appearance::Dark) => (DARK_CHROME_TINT, MICA_DARK_ALPHA),
        (WindowMaterial::Mica, Appearance::Light) => (LIGHT_CHROME_TINT, MICA_LIGHT_ALPHA),
        (WindowMaterial::Glass, Appearance::Dark) => (DARK_CHROME_TINT, GLASS_DARK_ALPHA),
        (WindowMaterial::Glass, Appearance::Light) => (LIGHT_CHROME_TINT, GLASS_LIGHT_ALPHA),
    }
}

pub(crate) fn window_backdrop(pal: &Palette, cx: &App) -> Background {
    let material = window_material(cx);
    if !material.is_translucent() {
        return backdrop(pal);
    }
    let base = chrome_tint(material, appearance(cx));
    let (tint, alpha) = tint::picked_chrome_tint(base, material, pal, cx);
    tinted(tint, alpha).into()
}

pub(crate) fn island_fill(pal: &Palette, cx: &App) -> Rgba {
    if window_material(cx).is_translucent() {
        tinted(pal.bg_panel, CLEAR_ALPHA)
    } else {
        rgb(pal.bg_panel)
    }
}

pub(crate) fn surface_fill(color: u32, cx: &App) -> Rgba {
    if !window_material(cx).is_translucent() {
        return rgb(color);
    }
    let alpha = match appearance(cx) {
        Appearance::Dark => SURFACE_DARK_ALPHA,
        Appearance::Light => SURFACE_LIGHT_ALPHA,
    };
    tinted(color, alpha)
}

pub(crate) fn popover_shadow(pal: &Palette, cx: &App) -> Vec<BoxShadow> {
    if window_material(cx).is_translucent() {
        return Vec::new();
    }
    floating_shadow(pal)
}

pub(crate) fn scrim_fill(pal: &Palette, cx: &App) -> Tint {
    if window_material(cx).is_translucent() {
        return Tint::new(pal.scrim.color, TRANSLUCENT_SCRIM_ALPHA);
    }
    pal.scrim
}

pub(crate) fn popover_fill(pal: &Palette, cx: &App) -> Rgba {
    let alpha = if window_material(cx).is_translucent() {
        POPOVER_TRANSLUCENT_ALPHA
    } else {
        POPOVER_ALPHA
    };
    tinted(pal.bg_surface, alpha)
}

pub(crate) fn terminal_fill_alpha(background: u32, pal: &Palette, cx: &App) -> Option<u32> {
    if !window_material(cx).is_translucent() {
        return Some(OPAQUE_ALPHA);
    }
    (background != pal.bg_surface).then_some(THEMED_TERMINAL_ALPHA)
}

pub(crate) fn terminal_fill(background: u32, pal: &Palette, cx: &App) -> Option<Rgba> {
    terminal_fill_alpha(background, pal, cx).map(|alpha| tinted(background, alpha))
}

pub(crate) fn focus_ring(pal: &Palette) -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.0), px(0.0), tinted(pal.accent, RING_ALPHA).into())
            .spread_radius(px(FOCUS_RING_PX)),
    ]
}

pub(crate) fn card_shadow(pal: &Palette) -> Vec<BoxShadow> {
    pal.shadows.card.map(BoxShadow::from).to_vec()
}

pub(crate) fn row_shadow(pal: &Palette, cx: &App) -> Vec<BoxShadow> {
    if window_material(cx).is_translucent() {
        return Vec::new();
    }
    solid_row_shadow(pal)
}

pub(crate) fn solid_row_shadow(pal: &Palette) -> Vec<BoxShadow> {
    pal.shadows.row.map(BoxShadow::from).to_vec()
}

pub(crate) fn row_fill(pal: &Palette, cx: &App) -> Rgba {
    if window_material(cx).is_translucent() {
        return Tint::new(pal.bg_selected.color, TRANSLUCENT_ROW_ALPHA).into();
    }
    row_fill(pal, cx)
}

pub(crate) fn floating_shadow(pal: &Palette) -> Vec<BoxShadow> {
    vec![pal.shadows.floating.into()]
}

fn style_kit_theme(theme: &mut Theme, pal: &Palette) {
    let color = |value: u32| -> Hsla { rgb(value).into() };
    theme.font_family = UI_FONT_FAMILY.into();
    theme.font_size = px(TEXT_SM);
    theme.mono_font_family = MONO_FONT_FAMILY.into();
    theme.mono_font_size = px(TEXT_SM);
    theme.background = color(pal.bg_surface);
    theme.foreground = color(pal.text_strong);
    theme.muted = pal.bg_hover.into();
    theme.muted_foreground = color(pal.text_subtle);
    theme.border = pal.border.into();
    theme.input = pal.border.into();
    theme.ring = color(pal.accent);
    theme.popover = color(pal.bg_surface);
    theme.popover_foreground = color(pal.text_strong);
    theme.accent = pal.bg_hover.into();
    theme.accent_foreground = color(pal.text_strong);
    theme.list_hover = pal.bg_hover.into();
    theme.list_active = pal.bg_selected.into();
    theme.secondary = color(pal.bg_surface);
    theme.secondary_hover = pal.bg_hover.into();
    theme.secondary_active = pal.bg_selected.into();
    theme.secondary_foreground = color(pal.text_strong);
    theme.title_bar = color(pal.bg_panel);
    theme.title_bar_border = pal.border.into();
    theme.danger = color(pal.error_fg);
    theme.success = color(pal.success_fg);
    theme.warning = color(pal.warning_fg);
    theme.info = color(pal.info_fg);
    theme.primary = color(pal.accent);
    theme.primary_hover = color(pal.accent_hover);
    theme.primary_active = color(pal.accent_pressed);
    theme.primary_foreground = color(pal.on_accent);
    theme.button_primary = color(pal.accent);
    theme.button_primary_hover = color(pal.accent_hover);
    theme.button_primary_active = color(pal.accent_pressed);
    theme.button_primary_foreground = color(pal.on_accent);
    theme.radius = px(RADIUS_SM);
    theme.radius_lg = px(RADIUS_LG);
}
