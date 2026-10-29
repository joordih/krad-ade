use gpui_kit::{App, Global};
use krad_domain::{Hue, TintOpacity, WindowMaterial, WindowTint};

use super::Palette;
use crate::window_frame::tint_adjustable;

const BLUE: u32 = 0x0040_8cff;
const PURPLE: u32 = 0x009b_6bf2;
const PINK: u32 = 0x00e8_5d9c;
const RED: u32 = 0x00e5_4d4d;
const ORANGE: u32 = 0x00f0_8a3c;
const YELLOW: u32 = 0x00e6_b829;
const GREEN: u32 = 0x003f_b96b;
const TEAL: u32 = 0x0026_b0b0;
const FULL_ALPHA: u32 = 0xff;
const HUE_SHARE: u32 = 18;
const PERCENT: u32 = 100;
const CHANNEL_BITS: u32 = 8;
const CHANNEL_MASK: u32 = 0xff;
const CHANNELS: [u32; 3] = [2 * CHANNEL_BITS, CHANNEL_BITS, 0];

pub(crate) const fn hue_rgb(hue: Hue, pal: &Palette) -> u32 {
    match hue {
        Hue::Blue => BLUE,
        Hue::Purple => PURPLE,
        Hue::Pink => PINK,
        Hue::Red => RED,
        Hue::Orange => ORANGE,
        Hue::Yellow => YELLOW,
        Hue::Green => GREEN,
        Hue::Teal => TEAL,
        Hue::Gray => pal.text_muted,
    }
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

const fn opacity_alpha(opacity: TintOpacity) -> u32 {
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
        assert_eq!(mix(base, YELLOW, 0), base);
        assert_eq!(mix(base, YELLOW, PERCENT), YELLOW);
        let tinted = mix(base, YELLOW, HUE_SHARE);
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
