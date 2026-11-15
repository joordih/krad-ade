use gpui_kit::{App, Global};
use krad_domain::{Hue, TintOpacity, WindowMaterial, WindowTint};

use super::Palette;
use crate::window_frame::tint_adjustable;

const FULL_ALPHA: u32 = 0xff;
const HUE_SHARE: u32 = 18;
const PERCENT: u32 = 100;
const CHANNEL_BITS: u32 = 8;
const CHANNEL_MASK: u32 = 0xff;
const CHANNELS: [u32; 3] = [2 * CHANNEL_BITS, CHANNEL_BITS, 0];

pub(crate) const fn hue_rgb(hue: Hue, pal: &Palette) -> u32 {
    krad_palette::hue_rgb(hue, pal.text_muted)
}

struct CurrentTint(WindowTint);

impl Global for CurrentTint {}

pub fn apply_window_tint(tint: WindowTint, cx: &mut App) {
    cx.set_global(CurrentTint(tint));
    cx.refresh_windows();
}

const fn mix(base: u32, hue: u32, share: u32) -> u32 {
    let mut mixed = 0;
    let mut index = 0;
    while index < CHANNELS.len() {
        let shift = CHANNELS[index];
        let from = (base >> shift) & CHANNEL_MASK;
        let to = (hue >> shift) & CHANNEL_MASK;
        let channel = (from * (PERCENT - share) + to * share) / PERCENT;
        mixed |= channel << shift;
        index += 1;
    }
    mixed
}

pub(super) const fn opacity_alpha(opacity: TintOpacity) -> u32 {
    opacity.percent() as u32 * FULL_ALPHA / TintOpacity::MAX as u32
}

pub(super) fn picked_chrome_tint(
    base: (u32, u32),
    material: WindowMaterial,
    pal: &Palette,
    cx: &App,
) -> (u32, u32) {
    let Some(CurrentTint(tint)) = cx.try_global::<CurrentTint>() else {
        return base;
    };
    if !tint_adjustable(material) {
        return base;
    }
    (
        tint.hue
            .map_or(base.0, |hue| mix(base.0, hue_rgb(hue, pal), HUE_SHARE)),
        tint.opacity.map_or(base.1, opacity_alpha),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hue_only_colours_the_neutral_tint() {
        let base = 0x0017_1717;
        let yellow = krad_palette::hue_rgb(Hue::Yellow, base);
        assert_eq!(mix(base, yellow, 0), base);
        assert_eq!(mix(base, yellow, PERCENT), yellow);
        let tinted = mix(base, yellow, HUE_SHARE);
        let red = (tinted >> CHANNELS[0]) & CHANNEL_MASK;
        assert!(red > 0x17 && red < 0x50, "{tinted:06x}");
    }

    #[test]
    fn opacity_spans_the_whole_alpha() {
        assert_eq!(opacity_alpha(TintOpacity::new(0)), 0);
        assert_eq!(
            opacity_alpha(TintOpacity::new(TintOpacity::MAX)),
            FULL_ALPHA
        );
    }
}
