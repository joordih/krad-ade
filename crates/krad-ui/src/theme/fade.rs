use gpui_kit::{App, Global};
use krad_session::AnsiPalette;
use std::collections::HashMap;
use std::time::Instant;

use super::{Palette, ShadowLayer, Shadows, SyntaxColors, Tint};
use crate::motion::{Easing, Tween, THEME_FADE};

const FADE_STEPS: u16 = 24;
const START: f32 = 0.0;
const END: f32 = 1.0;
const HALF: f32 = 0.5;
const CHANNEL_SHIFTS: [u32; 3] = [16, 8, 0];
const CHANNEL_MASK: u32 = 0xff;

type PalettePair = (usize, usize);

#[derive(Default)]
struct FadeFrames(HashMap<PalettePair, &'static [Palette]>);

impl Global for FadeFrames {}

#[derive(Clone, Copy)]
struct Frames {
    from: &'static Palette,
    to: &'static Palette,
    between: &'static [Palette],
}

impl Frames {
    fn at(&self, position: f32) -> &'static Palette {
        let step = (1..=FADE_STEPS)
            .take_while(|step| step_share(*step) - HALF / f32::from(FADE_STEPS) <= position)
            .count();
        match step {
            0 => self.from,
            step if step >= usize::from(FADE_STEPS) => self.to,
            step => self.between.get(step - 1).unwrap_or(self.to),
        }
    }
}

struct ThemeFade {
    frames: Frames,
    position: Tween,
}

impl Global for ThemeFade {}

fn step_share(step: u16) -> f32 {
    f32::from(step) / f32::from(FADE_STEPS)
}

fn key(from: &Palette, to: &Palette) -> PalettePair {
    (
        std::ptr::from_ref(from).addr(),
        std::ptr::from_ref(to).addr(),
    )
}

fn frames_between(from: &'static Palette, to: &'static Palette, cx: &mut App) -> Frames {
    let cache = cx.default_global::<FadeFrames>();
    let between = *cache.0.entry(key(from, to)).or_insert_with(|| {
        let steps: Vec<Palette> = (1..FADE_STEPS)
            .map(|step| mix_palette(from, to, Share::new(step)))
            .collect();
        Box::leak(steps.into_boxed_slice())
    });
    Frames { from, to, between }
}

pub(super) fn begin(
    shown: Option<&'static Palette>,
    target: &'static Palette,
    now: Instant,
    cx: &mut App,
) -> &'static Palette {
    let Some(shown) = shown.filter(|shown| !std::ptr::eq(*shown, target)) else {
        end_fade(cx);
        return target;
    };
    if cx.reduce_motion() {
        end_fade(cx);
        return target;
    }
    let running = cx
        .try_global::<ThemeFade>()
        .map(|fade| (fade.frames, fade.position));
    let (frames, mut position) = match running {
        Some((frames, position)) if std::ptr::eq(frames.to, target) => (frames, position),
        Some((frames, position)) if std::ptr::eq(frames.from, target) => (frames, position),
        Some((frames, position)) => {
            let nearest = if position.value(now) < HALF {
                frames.from
            } else {
                frames.to
            };
            (frames_between(nearest, target, cx), Tween::at_rest(START))
        }
        None => (frames_between(shown, target, cx), Tween::at_rest(START)),
    };
    let destination = if std::ptr::eq(frames.from, target) {
        START
    } else {
        END
    };
    position.retarget(destination, THEME_FADE, Easing::OutCubic, now);
    cx.set_global(ThemeFade { frames, position });
    frames.at(position.value(now))
}

pub(super) fn advance(now: Instant, cx: &mut App) -> Option<(&'static Palette, bool)> {
    if !cx.has_global::<ThemeFade>() {
        return None;
    }
    let reduce = cx.reduce_motion();
    let fade = cx.global_mut::<ThemeFade>();
    if reduce {
        fade.position.settle();
    }
    let moving = fade.position.advance(now);
    let shown = fade.frames.at(fade.position.value(now));
    if !moving {
        cx.remove_global::<ThemeFade>();
    }
    Some((shown, moving))
}

fn end_fade(cx: &mut App) {
    if cx.has_global::<ThemeFade>() {
        cx.remove_global::<ThemeFade>();
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Share {
    step: u16,
}

impl Share {
    pub(super) const fn new(step: u16) -> Self {
        Self {
            step: if step > FADE_STEPS { FADE_STEPS } else { step },
        }
    }

    fn fraction(self) -> f32 {
        step_share(self.step)
    }

    const fn channel(self, from: u32, to: u32) -> u32 {
        let steps = FADE_STEPS as u32;
        let step = self.step as u32;
        (from * (steps - step) + to * step + steps / 2) / steps
    }

    pub(super) const fn rgb(self, from: u32, to: u32) -> u32 {
        let mut mixed = 0;
        let mut index = 0;
        while index < CHANNEL_SHIFTS.len() {
            let shift = CHANNEL_SHIFTS[index];
            let channel =
                self.channel((from >> shift) & CHANNEL_MASK, (to >> shift) & CHANNEL_MASK);
            mixed |= channel << shift;
            index += 1;
        }
        mixed
    }
}

fn mix_f32(from: f32, to: f32, share: Share) -> f32 {
    from + (to - from) * share.fraction()
}

fn mix_tint(from: Tint, to: Tint, share: Share) -> Tint {
    Tint::new(
        share.rgb(from.color, to.color),
        mix_f32(from.alpha, to.alpha, share),
    )
}

fn mix_layer(from: ShadowLayer, to: ShadowLayer, share: Share) -> ShadowLayer {
    ShadowLayer {
        y: mix_f32(from.y, to.y, share),
        blur: mix_f32(from.blur, to.blur, share),
        spread: mix_f32(from.spread, to.spread, share),
        tint: mix_tint(from.tint, to.tint, share),
    }
}

fn mix_shadows(from: &Shadows, to: &Shadows, share: Share) -> Shadows {
    let pair = |from: [ShadowLayer; 2], to: [ShadowLayer; 2]| {
        [
            mix_layer(from[0], to[0], share),
            mix_layer(from[1], to[1], share),
        ]
    };
    Shadows {
        card: pair(from.card, to.card),
        row: pair(from.row, to.row),
        floating: mix_layer(from.floating, to.floating, share),
    }
}

fn mix_syntax(from: &SyntaxColors, to: &SyntaxColors, share: Share) -> SyntaxColors {
    let mix = |from: u32, to: u32| share.rgb(from, to);
    SyntaxColors {
        keyword: mix(from.keyword, to.keyword),
        string: mix(from.string, to.string),
        comment: mix(from.comment, to.comment),
        number: mix(from.number, to.number),
        type_name: mix(from.type_name, to.type_name),
    }
}

fn mix_ansi(from: &AnsiPalette, to: &AnsiPalette, share: Share) -> AnsiPalette {
    let mut mixed = from.0;
    mixed
        .iter_mut()
        .zip(to.0)
        .for_each(|(color, target)| *color = share.rgb(*color, target));
    AnsiPalette(mixed)
}

fn mix_palette(from: &Palette, to: &Palette, share: Share) -> Palette {
    let rgb = |from: u32, to: u32| share.rgb(from, to);
    let tint = |from: Tint, to: Tint| mix_tint(from, to, share);
    Palette {
        backdrop_from: rgb(from.backdrop_from, to.backdrop_from),
        backdrop_to: rgb(from.backdrop_to, to.backdrop_to),
        shell_ink: rgb(from.shell_ink, to.shell_ink),
        text_strong: rgb(from.text_strong, to.text_strong),
        text_default: rgb(from.text_default, to.text_default),
        text_subtle: rgb(from.text_subtle, to.text_subtle),
        text_muted: rgb(from.text_muted, to.text_muted),
        text_disabled: rgb(from.text_disabled, to.text_disabled),
        bg_surface: rgb(from.bg_surface, to.bg_surface),
        bg_row_selected: rgb(from.bg_row_selected, to.bg_row_selected),
        bg_panel: rgb(from.bg_panel, to.bg_panel),
        bg_hover: tint(from.bg_hover, to.bg_hover),
        bg_selected: tint(from.bg_selected, to.bg_selected),
        border: tint(from.border, to.border),
        divider: tint(from.divider, to.divider),
        scrim: tint(from.scrim, to.scrim),
        attention: rgb(from.attention, to.attention),
        attention_text: rgb(from.attention_text, to.attention_text),
        chip_bg: rgb(from.chip_bg, to.chip_bg),
        chip_fg: rgb(from.chip_fg, to.chip_fg),
        accent: rgb(from.accent, to.accent),
        accent_hover: rgb(from.accent_hover, to.accent_hover),
        accent_pressed: rgb(from.accent_pressed, to.accent_pressed),
        on_accent: rgb(from.on_accent, to.on_accent),
        success_fg: rgb(from.success_fg, to.success_fg),
        warning_fg: rgb(from.warning_fg, to.warning_fg),
        error_fg: rgb(from.error_fg, to.error_fg),
        info_fg: rgb(from.info_fg, to.info_fg),
        diff_added: rgb(from.diff_added, to.diff_added),
        diff_removed: rgb(from.diff_removed, to.diff_removed),
        diff_modified: rgb(from.diff_modified, to.diff_modified),
        ansi: mix_ansi(&from.ansi, &to.ansi, share),
        syntax: mix_syntax(&from.syntax, &to.syntax, share),
        shadows: mix_shadows(&from.shadows, &to.shadows, share),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Appearance;

    const BLACK: u32 = 0x0000_0000;
    const WHITE: u32 = 0x00ff_ffff;

    fn endpoints() -> Frames {
        let (from, to) = (Appearance::Light.palette(), Appearance::Dark.palette());
        let between: Vec<Palette> = (1..FADE_STEPS)
            .map(|step| mix_palette(from, to, Share::new(step)))
            .collect();
        Frames {
            from,
            to,
            between: Box::leak(between.into_boxed_slice()),
        }
    }

    #[test]
    fn rgb_mixing_lands_on_both_ends_and_moves_each_channel_alone() {
        assert_eq!(Share::new(0).rgb(BLACK, WHITE), BLACK);
        assert_eq!(Share::new(FADE_STEPS).rgb(BLACK, WHITE), WHITE);
        assert_eq!(Share::new(FADE_STEPS / 2).rgb(BLACK, WHITE), 0x0080_8080);
        assert_eq!(
            Share::new(FADE_STEPS / 2).rgb(0x00ff_0000, 0x0000_00ff),
            0x0080_0080
        );
        assert_eq!(Share::new(FADE_STEPS * 2), Share::new(FADE_STEPS));
    }

    #[test]
    fn rgb_mixing_is_monotonic_along_the_fade() {
        let reds: Vec<u32> = (0..=FADE_STEPS)
            .map(|step| Share::new(step).rgb(0x0010_2030, 0x00f0_e0d0) >> 16)
            .collect();
        assert!(reds.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn a_mixed_palette_sits_between_its_ends() {
        let (light, dark) = (Appearance::Light.palette(), Appearance::Dark.palette());
        let middle = mix_palette(light, dark, Share::new(FADE_STEPS / 2));
        let (from, to) = (light.bg_surface & 0xff, dark.bg_surface & 0xff);
        let mid = middle.bg_surface & 0xff;
        assert!(mid < from.max(to) && mid > from.min(to), "{mid:x}");
        let alpha = middle.scrim.alpha;
        assert!(alpha >= light.scrim.alpha.min(dark.scrim.alpha));
        assert!(alpha <= light.scrim.alpha.max(dark.scrim.alpha));
    }

    #[test]
    fn frames_show_the_ends_at_the_ends_and_step_through_the_middle() {
        let frames = endpoints();
        assert!(std::ptr::eq(frames.at(START), frames.from));
        assert!(std::ptr::eq(frames.at(END), frames.to));
        assert!(std::ptr::eq(frames.at(END + 1.0), frames.to));
        let middle = frames.at(HALF);
        assert!(!std::ptr::eq(middle, frames.from) && !std::ptr::eq(middle, frames.to));
        let expected = &frames.between[usize::from(FADE_STEPS / 2) - 1];
        assert!(std::ptr::eq(middle, expected));
    }
}
