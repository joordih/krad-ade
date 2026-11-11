const CHANNEL_SHIFTS: [u32; 3] = [16, 8, 0];
const CHANNEL_MASK: u32 = 0xff;
const CHANNEL_MAX: f32 = 255.0;
const FULL_TURN: f32 = 360.0;
const SECTOR: f32 = 60.0;
const HALF: f32 = 0.5;
const SRGB_KNEE: f32 = 0.039_28;
const SRGB_LINEAR_SLOPE: f32 = 12.92;
const SRGB_OFFSET: f32 = 0.055;
const SRGB_SCALE: f32 = 1.055;
const SRGB_GAMMA: f32 = 2.4;
const LUMA_RED: f32 = 0.2126;
const LUMA_GREEN: f32 = 0.7152;
const LUMA_BLUE: f32 = 0.0722;
const CONTRAST_FLARE: f32 = 0.05;
const WHITE: u32 = 0x00ff_ffff;
const BLACK: u32 = 0x0000_0000;
const READABLE_STEPS: u16 = 20;

pub(crate) const AA_BODY_TEXT: f32 = 4.5;
pub(crate) const AA_LARGE_TEXT: f32 = 3.0;

fn channels(color: u32) -> [f32; 3] {
    CHANNEL_SHIFTS.map(|shift| {
        let byte = u8::try_from((color >> shift) & CHANNEL_MASK).unwrap_or(u8::MAX);
        f32::from(byte) / CHANNEL_MAX
    })
}

fn pack(values: [f32; 3]) -> u32 {
    values
        .iter()
        .zip(CHANNEL_SHIFTS)
        .fold(0, |packed, (value, shift)| {
            packed | (u32::from(channel_byte(*value)) << shift)
        })
}

fn channel_byte(value: f32) -> u8 {
    let scaled = (value.clamp(0.0, 1.0) * CHANNEL_MAX).round();
    (0..=u8::MAX)
        .find(|byte| f32::from(*byte) >= scaled)
        .unwrap_or(u8::MAX)
}

pub(crate) fn hsl_rgb(hue: f32, saturation: f32, lightness: f32) -> u32 {
    let (saturation, lightness) = (saturation.clamp(0.0, 1.0), lightness.clamp(0.0, 1.0));
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(FULL_TURN) / SECTOR;
    let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let base = lightness - chroma / 2.0;
    let [red, green, blue] = match sector {
        sector if sector < 1.0 => [chroma, second, 0.0],
        sector if sector < 2.0 => [second, chroma, 0.0],
        sector if sector < 3.0 => [0.0, chroma, second],
        sector if sector < 4.0 => [0.0, second, chroma],
        sector if sector < 5.0 => [second, 0.0, chroma],
        _ => [chroma, 0.0, second],
    };
    pack([red + base, green + base, blue + base])
}

pub(crate) fn hsl_lightness(color: u32) -> f32 {
    let values = channels(color);
    let high = values.iter().copied().fold(0.0, f32::max);
    let low = values.iter().copied().fold(1.0, f32::min);
    (high + low) * HALF
}

fn relative_luminance(color: u32) -> f32 {
    let [red, green, blue] = channels(color).map(|value| {
        if value <= SRGB_KNEE {
            value / SRGB_LINEAR_SLOPE
        } else {
            ((value + SRGB_OFFSET) / SRGB_SCALE).powf(SRGB_GAMMA)
        }
    });
    LUMA_RED * red + LUMA_GREEN * green + LUMA_BLUE * blue
}

pub(crate) fn contrast(first: u32, second: u32) -> f32 {
    let (a, b) = (relative_luminance(first), relative_luminance(second));
    (a.max(b) + CONTRAST_FLARE) / (a.min(b) + CONTRAST_FLARE)
}

fn worst_contrast(text: u32, backgrounds: &[u32]) -> f32 {
    backgrounds
        .iter()
        .map(|background| contrast(text, *background))
        .fold(f32::INFINITY, f32::min)
}

pub(crate) fn mix(from: u32, to: u32, share: f32) -> u32 {
    let (from, to) = (channels(from), channels(to));
    let share = share.clamp(0.0, 1.0);
    pack([0, 1, 2].map(|index| from[index] + (to[index] - from[index]) * share))
}

pub(crate) fn readable(text: u32, backgrounds: &[u32], minimum: f32) -> u32 {
    if worst_contrast(text, backgrounds) >= minimum {
        return text;
    }
    let toward = [WHITE, BLACK]
        .into_iter()
        .max_by(|a, b| worst_contrast(*a, backgrounds).total_cmp(&worst_contrast(*b, backgrounds)))
        .unwrap_or(WHITE);
    (1..=READABLE_STEPS)
        .map(|step| mix(text, toward, f32::from(step) / f32::from(READABLE_STEPS)))
        .find(|candidate| worst_contrast(*candidate, backgrounds) >= minimum)
        .unwrap_or(toward)
}

pub(crate) fn clearest_on(background: u32, current: u32, minimum: f32) -> u32 {
    if contrast(current, background) >= minimum {
        return current;
    }
    [WHITE, BLACK, current]
        .into_iter()
        .max_by(|a, b| contrast(*a, background).total_cmp(&contrast(*b, background)))
        .unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_hits_the_primaries_greys_and_wraps_the_hue() {
        assert_eq!(hsl_rgb(0.0, 1.0, 0.5), 0x00ff_0000);
        assert_eq!(hsl_rgb(120.0, 1.0, 0.5), 0x0000_ff00);
        assert_eq!(hsl_rgb(240.0, 1.0, 0.5), 0x0000_00ff);
        assert_eq!(hsl_rgb(60.0, 1.0, 0.5), 0x00ff_ff00);
        assert_eq!(hsl_rgb(360.0, 1.0, 0.5), hsl_rgb(0.0, 1.0, 0.5));
        assert_eq!(hsl_rgb(200.0, 0.0, 0.0), BLACK);
        assert_eq!(hsl_rgb(200.0, 0.0, 1.0), WHITE);
        assert_eq!(hsl_rgb(17.0, 0.0, 0.09), 0x0017_1717);
        assert_eq!(hsl_rgb(0.0, 2.0, 0.5), 0x00ff_0000, "saturation is capped");
    }

    #[test]
    fn lightness_reads_back_what_hsl_wrote() {
        for lightness in [0.0, 0.09, 0.3, 0.5, 0.91, 1.0] {
            let read = hsl_lightness(hsl_rgb(210.0, 0.4, lightness));
            assert!(
                (read - lightness).abs() < 0.01,
                "{lightness} read as {read}"
            );
        }
    }

    #[test]
    fn contrast_runs_from_one_to_twenty_one() {
        assert!((contrast(WHITE, BLACK) - 21.0).abs() < 0.01);
        assert!((contrast(0x0080_8080, 0x0080_8080) - 1.0).abs() < f32::EPSILON);
        assert!((contrast(WHITE, BLACK) - contrast(BLACK, WHITE)).abs() < f32::EPSILON);
    }

    #[test]
    fn unreadable_text_moves_until_it_passes_on_every_background() {
        let backgrounds = [0x0060_6060, 0x0070_7070];
        let text = readable(0x0090_9090, &backgrounds, AA_BODY_TEXT);
        assert!(backgrounds
            .iter()
            .all(|background| contrast(text, *background) >= AA_BODY_TEXT));
        assert_eq!(readable(WHITE, &[BLACK], AA_BODY_TEXT), WHITE);
    }

    #[test]
    fn text_on_a_mid_grey_takes_the_clearer_extreme_when_nothing_passes() {
        let text = readable(0x0077_7777, &[0x0000_0000, 0x00ff_ffff], AA_BODY_TEXT);
        assert!(text == WHITE || text == BLACK);
    }

    #[test]
    fn labels_on_an_accent_flip_only_when_unreadable() {
        assert_eq!(clearest_on(0x0000_7aff, WHITE, AA_LARGE_TEXT), WHITE);
        assert_eq!(clearest_on(0x00ff_e066, WHITE, AA_LARGE_TEXT), BLACK);
    }
}
