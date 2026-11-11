mod color_math;

use gpui_kit::{App, Global};
use krad_domain::{
    AccentChoice, DiffColors, DiffTriple, MainPaneMaterial, Percent, SurfaceTone, ThemeColor,
    ThemeTuning,
};
use std::collections::HashMap;

pub(crate) use color_math::hsl_rgb;
use color_math::{
    clearest_on, contrast, hsl_lightness, mix, readable, AA_BODY_TEXT, AA_LARGE_TEXT,
};

use super::{hue_rgb, Appearance, Palette};

const WHITE: u32 = 0x00ff_ffff;
const BLACK: u32 = 0x0000_0000;
const ACCENT_SHADE: f32 = 0.15;
const WHOLE: f32 = 100.0;
const LIGHTNESS_STEP: f32 = 0.01;
const LIGHTNESS_STEPS: u16 = 100;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(super) struct PaletteTuning {
    accent: AccentChoice,
    diff: DiffColors,
    surfaces: SurfaceTone,
}

impl From<&ThemeTuning> for PaletteTuning {
    fn from(tuning: &ThemeTuning) -> Self {
        Self {
            accent: tuning.accent,
            diff: tuning.diff,
            surfaces: tuning.surfaces,
        }
    }
}

impl PaletteTuning {
    fn is_neutral(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Default)]
pub(super) struct CurrentTuning(pub(super) ThemeTuning);

impl Global for CurrentTuning {}

#[derive(Default)]
struct TunedPalettes(HashMap<(usize, PaletteTuning), &'static Palette>);

impl Global for TunedPalettes {}

pub(super) fn current_tuning(cx: &App) -> ThemeTuning {
    cx.try_global::<CurrentTuning>()
        .map_or_else(ThemeTuning::default, |current| current.0)
}

pub(crate) fn main_pane_material(cx: &App) -> MainPaneMaterial {
    current_tuning(cx).main_pane
}

pub(crate) fn sidebar_opacity(cx: &App) -> Option<Percent> {
    current_tuning(cx).sidebar_opacity
}

pub(super) fn tuned_palette(
    base: &'static Palette,
    appearance: Appearance,
    cx: &mut App,
) -> &'static Palette {
    let tuning = PaletteTuning::from(&current_tuning(cx));
    if tuning.is_neutral() {
        return base;
    }
    let key = (std::ptr::from_ref(base).addr(), tuning);
    let cache = cx.default_global::<TunedPalettes>();
    if let Some(palette) = cache.0.get(&key) {
        return palette;
    }
    let palette: &'static Palette = Box::leak(Box::new(tune(base, tuning, appearance)));
    cache.0.insert(key, palette);
    palette
}

fn tone(color: u32, tone: SurfaceTone, shift: f32, ink: u32, appearance: Appearance) -> u32 {
    let (hue, saturation) = (f32::from(tone.hue.value()), tone.saturation.fraction());
    let start = hsl_lightness(color) + shift;
    let away = match appearance {
        Appearance::Dark => -LIGHTNESS_STEP,
        Appearance::Light => LIGHTNESS_STEP,
    };
    (0..=LIGHTNESS_STEPS)
        .map(|step| hsl_rgb(hue, saturation, start + away * f32::from(step)))
        .find(|surface| contrast(ink, *surface) >= AA_BODY_TEXT)
        .unwrap_or(match appearance {
            Appearance::Dark => BLACK,
            Appearance::Light => WHITE,
        })
}

fn tone_surfaces(palette: &mut Palette, surfaces: SurfaceTone, appearance: Appearance) {
    let shift = match appearance {
        Appearance::Dark => {
            f32::from(surfaces.dark_lightness.value()) / WHOLE - hsl_lightness(palette.shell_ink)
        }
        Appearance::Light => 0.0,
    };
    let ink = palette.text_default;
    for color in [
        &mut palette.backdrop_from,
        &mut palette.backdrop_to,
        &mut palette.shell_ink,
        &mut palette.bg_surface,
        &mut palette.bg_row_selected,
        &mut palette.bg_panel,
        &mut palette.chip_bg,
    ] {
        *color = tone(*color, surfaces, shift, ink, appearance);
    }
}

fn keep_text_readable(palette: &mut Palette) {
    let backgrounds = [
        palette.bg_surface,
        palette.bg_panel,
        palette.bg_row_selected,
        palette.backdrop_from,
        palette.backdrop_to,
    ];
    for text in [
        &mut palette.text_strong,
        &mut palette.text_default,
        &mut palette.text_subtle,
        &mut palette.text_muted,
    ] {
        *text = readable(*text, &backgrounds, AA_BODY_TEXT);
    }
    palette.chip_fg = readable(palette.chip_fg, &[palette.chip_bg], AA_BODY_TEXT);
}

fn accent_color(choice: AccentChoice, base: &Palette) -> Option<u32> {
    match choice {
        AccentChoice::Theme => None,
        AccentChoice::Swatch(hue) => Some(hue_rgb(hue, base)),
        AccentChoice::Custom(ThemeColor(color)) => Some(color),
    }
}

fn paint_accent(palette: &mut Palette, accent: u32) {
    palette.accent = accent;
    palette.accent_hover = mix(accent, WHITE, ACCENT_SHADE);
    palette.accent_pressed = mix(accent, BLACK, ACCENT_SHADE);
    palette.on_accent = clearest_on(accent, palette.on_accent, AA_LARGE_TEXT);
}

pub(super) fn theme_diff(palette: &Palette) -> DiffTriple {
    DiffTriple {
        added: ThemeColor(palette.success_fg),
        removed: ThemeColor(palette.error_fg),
        modified: ThemeColor(palette.warning_fg),
    }
}

fn paint_diff(palette: &mut Palette, diff: &DiffColors, appearance: Appearance) {
    let resolved = diff.resolve(appearance.into(), theme_diff(palette));
    palette.diff_added = resolved.added.0;
    palette.diff_removed = resolved.removed.0;
    palette.diff_modified = resolved.modified.0;
}

fn tune(base: &Palette, tuning: PaletteTuning, appearance: Appearance) -> Palette {
    let mut palette = base.clone();
    if !tuning.surfaces.is_default() {
        tone_surfaces(&mut palette, tuning.surfaces, appearance);
        keep_text_readable(&mut palette);
    }
    if let Some(accent) = accent_color(tuning.accent, base) {
        paint_accent(&mut palette, accent);
    }
    if !tuning.diff.is_default() {
        paint_diff(&mut palette, &tuning.diff, appearance);
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{DarkLightness, DiffPreset, Hue, HueDegrees};

    const SIDES: [Appearance; 2] = [Appearance::Light, Appearance::Dark];

    fn toned(hue: u16, saturation: u8, lightness: u8) -> PaletteTuning {
        PaletteTuning {
            surfaces: SurfaceTone {
                hue: HueDegrees::new(hue),
                saturation: Percent::new(saturation),
                dark_lightness: DarkLightness::new(lightness),
            },
            ..PaletteTuning::default()
        }
    }

    fn surfaces(palette: &Palette) -> [u32; 5] {
        [
            palette.bg_surface,
            palette.bg_panel,
            palette.bg_row_selected,
            palette.backdrop_from,
            palette.backdrop_to,
        ]
    }

    #[test]
    fn neutral_tuning_leaves_the_palette_alone() {
        for side in SIDES {
            let base = side.palette();
            let same = tune(base, PaletteTuning::default(), side);
            assert_eq!(surfaces(&same), surfaces(base));
            assert_eq!(same.accent, base.accent);
            assert_eq!(same.text_strong, base.text_strong);
        }
    }

    #[test]
    fn the_default_dark_lightness_keeps_the_shell_ink_near_nine_percent() {
        let dark = tune(
            Appearance::Dark.palette(),
            toned(220, 0, 9),
            Appearance::Dark,
        );
        assert_eq!(dark.shell_ink, 0x0017_1717);
        let lifted = tune(
            Appearance::Dark.palette(),
            toned(220, 0, 20),
            Appearance::Dark,
        );
        assert!(hsl_lightness(lifted.bg_surface) > hsl_lightness(dark.bg_surface));
        assert!(hsl_lightness(lifted.bg_panel) > hsl_lightness(lifted.bg_surface));
    }

    #[test]
    fn a_hue_tints_the_surfaces_toward_it() {
        let blue = tune(
            Appearance::Dark.palette(),
            toned(220, 60, 9),
            Appearance::Dark,
        );
        let [red, _, blue_channel] = [16, 8, 0].map(|shift| (blue.bg_panel >> shift) & 0xff);
        assert!(blue_channel > red, "{:06x}", blue.bg_panel);
    }

    #[test]
    fn body_text_stays_readable_for_every_slider_value() {
        for side in SIDES {
            for hue in (0..=HueDegrees::MAX).step_by(45) {
                for saturation in (0..=Percent::MAX).step_by(25) {
                    for lightness in DarkLightness::MIN..=DarkLightness::MAX {
                        let palette = tune(side.palette(), toned(hue, saturation, lightness), side);
                        for background in surfaces(&palette) {
                            for text in [palette.text_strong, palette.text_default] {
                                assert!(
                                    contrast(text, background) >= AA_BODY_TEXT,
                                    "{side:?} {hue} {saturation} {lightness}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn an_accent_swatch_or_custom_colour_drives_the_accent_and_keeps_its_label_readable() {
        let base = Appearance::Light.palette();
        let custom = PaletteTuning {
            accent: AccentChoice::Custom(ThemeColor(0x00ff_e066)),
            ..PaletteTuning::default()
        };
        let yellow = tune(base, custom, Appearance::Light);
        assert_eq!(yellow.accent, 0x00ff_e066);
        assert!(contrast(yellow.on_accent, yellow.accent) >= AA_LARGE_TEXT);
        assert_ne!(yellow.accent_hover, yellow.accent);
        let swatch = PaletteTuning {
            accent: AccentChoice::Swatch(Hue::Green),
            ..PaletteTuning::default()
        };
        assert_eq!(
            tune(base, swatch, Appearance::Light).accent,
            hue_rgb(Hue::Green, base)
        );
    }

    #[test]
    fn a_diff_preset_paints_the_diff_colours_only() {
        let base = Appearance::Dark.palette();
        let preset = PaletteTuning {
            diff: DiffColors {
                preset: DiffPreset::ColorBlind,
                ..DiffColors::default()
            },
            ..PaletteTuning::default()
        };
        let tuned = tune(base, preset, Appearance::Dark);
        assert_ne!(tuned.diff_added, base.diff_added);
        assert_eq!(tuned.success_fg, base.success_fg);
        assert_eq!(base.diff_added, base.success_fg);
        assert_eq!(base.diff_removed, base.error_fg);
        assert_eq!(base.diff_modified, base.warning_fg);
    }
}
