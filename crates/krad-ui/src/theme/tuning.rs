use gpui_kit::{App, Global};
use krad_domain::{MainPaneMaterial, Percent, ThemeTuning};
use krad_palette::PaletteTuning;
use std::collections::HashMap;

use super::{Appearance, Palette};

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
    let tuned = krad_palette::tune(&base.into(), tuning, appearance.into());
    let palette: &'static Palette = Box::leak(Box::new(Palette::from(&tuned)));
    cache.0.insert(key, palette);
    palette
}
