use gpui_kit::{rgb, App, Rgba};
use krad_domain::{BackdropPercent, TintOpacity, WindowMaterial};

use super::tint::opacity_alpha;
use super::tuning::{main_pane_material, sidebar_opacity};
use super::{appearance, tinted, window_material, Appearance, Palette};

const OPAQUE_INK: u32 = 0;

const OPAQUE: TintOpacity = TintOpacity::new(TintOpacity::MAX);
const CLEAR: TintOpacity = TintOpacity::new(0);
const SHELL_OPACITY: TintOpacity = TintOpacity::new(85);
const GLASS_DARK_OPACITY: TintOpacity = TintOpacity::new(65);
const GLASS_LIGHT_OPACITY: TintOpacity = TintOpacity::new(80);
const MICA_DARK_OPACITY: TintOpacity = CLEAR;
const MICA_LIGHT_OPACITY: TintOpacity = TintOpacity::new(80);
const PANE_OPACITY: TintOpacity = TintOpacity::new(28);
const INSET_OPACITY: TintOpacity = CLEAR;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SurfaceLevel {
    Shell,
    Pane,
    Inset,
}

impl SurfaceLevel {
    const fn opacity(self, material: WindowMaterial, appearance: Appearance) -> TintOpacity {
        if !material.is_translucent() {
            return OPAQUE;
        }
        match self {
            Self::Shell => shell_opacity(material, appearance),
            Self::Pane => PANE_OPACITY,
            Self::Inset => INSET_OPACITY,
        }
    }
}

pub(crate) const fn shell_opacity(material: WindowMaterial, appearance: Appearance) -> TintOpacity {
    match (material, appearance) {
        (WindowMaterial::Opaque, _) => OPAQUE,
        (WindowMaterial::Blurred, _) => SHELL_OPACITY,
        (WindowMaterial::Mica, Appearance::Dark) => MICA_DARK_OPACITY,
        (WindowMaterial::Mica, Appearance::Light) => MICA_LIGHT_OPACITY,
        (WindowMaterial::Glass, Appearance::Dark) => GLASS_DARK_OPACITY,
        (WindowMaterial::Glass, Appearance::Light) => GLASS_LIGHT_OPACITY,
    }
}

pub(super) const fn shell_tint(
    material: WindowMaterial,
    appearance: Appearance,
    pal: &Palette,
) -> (u32, u32) {
    let ink = if material.is_translucent() {
        pal.shell_ink
    } else {
        OPAQUE_INK
    };
    (
        ink,
        opacity_alpha(SurfaceLevel::Shell.opacity(material, appearance)),
    )
}

pub(crate) fn level_fill(level: SurfaceLevel, color: u32, cx: &App) -> Rgba {
    let material = window_material(cx);
    let opaque_pane = level == SurfaceLevel::Pane && !main_pane_material(cx).is_glass();
    if !material.is_translucent() || opaque_pane {
        return rgb(color);
    }
    tinted(
        color,
        opacity_alpha(level.opacity(material, appearance(cx))),
    )
}

pub(crate) fn sidebar_fill(pal: &Palette, cx: &App) -> Option<Rgba> {
    if !window_material(cx).is_translucent() {
        return None;
    }
    sidebar_opacity(cx).map(|opacity| {
        tinted(
            pal.shell_ink,
            opacity_alpha(TintOpacity::new(opacity.value())),
        )
    })
}

pub(crate) fn backdrop_veil(pal: &Palette, dim: BackdropPercent) -> Rgba {
    tinted(
        pal.backdrop_from,
        opacity_alpha(TintOpacity::new(dim.percent())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: [SurfaceLevel; 3] =
        [SurfaceLevel::Shell, SurfaceLevel::Pane, SurfaceLevel::Inset];
    const SIDES: [Appearance; 2] = [Appearance::Light, Appearance::Dark];

    #[test]
    fn the_backdrop_veil_takes_the_shell_color_at_the_dim_amount() {
        let dark = Appearance::Dark.palette();
        let clear = backdrop_veil(dark, BackdropPercent::new(0));
        let full = backdrop_veil(dark, BackdropPercent::new(BackdropPercent::MAX));
        assert!(clear.a.abs() < f32::EPSILON);
        assert!((full.a - 1.0).abs() < f32::EPSILON);
        assert_eq!(rgb(dark.backdrop_from).r, full.r);
    }

    #[test]
    fn an_opaque_window_paints_every_level_solid() {
        for level in LEVELS {
            for side in SIDES {
                assert_eq!(level.opacity(WindowMaterial::Opaque, side), OPAQUE);
            }
        }
    }

    #[test]
    fn glass_levels_thin_out_from_the_shell_to_the_inset() {
        for material in [WindowMaterial::Blurred, WindowMaterial::Glass] {
            for side in SIDES {
                let [shell, pane, inset] =
                    LEVELS.map(|level| level.opacity(material, side).percent());
                assert!(shell > pane, "{material:?} {side:?}");
                assert!(pane > inset, "{material:?} {side:?}");
                assert_eq!(inset, 0);
            }
        }
    }

    #[test]
    fn the_blurred_shell_covers_most_of_the_desktop_and_panes_let_it_through() {
        assert_eq!(
            shell_opacity(WindowMaterial::Blurred, Appearance::Dark).percent(),
            85
        );
        assert_eq!(PANE_OPACITY.percent(), 28);
    }

    #[test]
    fn the_dark_shell_ink_stays_near_nine_percent_luminosity() {
        let dark = Appearance::Dark.palette();
        let (ink, alpha) = shell_tint(WindowMaterial::Blurred, Appearance::Dark, dark);
        let luminosity = f64::from(ink & 0xff) / f64::from(0xff_u32);
        assert!((luminosity - 0.09).abs() < 0.01, "{luminosity}");
        assert_eq!(alpha, opacity_alpha(SHELL_OPACITY));
        let (_, solid) = shell_tint(WindowMaterial::Opaque, Appearance::Dark, dark);
        assert_eq!(solid, opacity_alpha(OPAQUE));
    }
}
