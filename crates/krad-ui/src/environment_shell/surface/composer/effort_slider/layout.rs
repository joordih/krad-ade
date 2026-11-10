use gpui_kit::Rgba;
use krad_domain::Effort;

pub(super) const POPOVER_W: f32 = 312.0;
pub(super) const TRACK_W: f32 = 280.0;
pub(super) const THUMB_W: f32 = 26.0;
pub(super) const DOT_PITCH: f32 = 7.0;
pub(super) const DOT_ROWS: usize = 3;
const EDGE_MARGIN: f32 = 8.0;
const MIN_TONE: f32 = 0.35;
const DOT_FLOOR: f32 = 0.12;
const DOT_SPREAD: f32 = 0.78;
const HASH_MIX_COL: u32 = 0x9E37_79B9;
const HASH_MIX_ROW: u32 = 0x85EB_CA6B;
const HASH_BUCKETS: u16 = 1000;
const DOT_ALPHAS: [u32; 6] = [0x30, 0x50, 0x70, 0x90, 0xB0, 0xD0];
const OVERSHOOT: f32 = 1.4;
const KEY_LEFT: &str = "left";
const KEY_RIGHT: &str = "right";
const KEY_DOWN: &str = "down";
const KEY_UP: &str = "up";
const KEY_HOME: &str = "home";
const KEY_END: &str = "end";
const KEY_ESCAPE: &str = "escape";
pub(super) const KEY_HELP: &str = "?";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlideKey {
    Lower,
    Higher,
    Lowest,
    Highest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PopoverKey {
    Close,
    Help,
    Slide(SlideKey),
}

pub(super) fn popover_key(key: &str) -> Option<PopoverKey> {
    match key {
        KEY_ESCAPE => Some(PopoverKey::Close),
        KEY_HELP => Some(PopoverKey::Help),
        KEY_LEFT | KEY_DOWN => Some(PopoverKey::Slide(SlideKey::Lower)),
        KEY_RIGHT | KEY_UP => Some(PopoverKey::Slide(SlideKey::Higher)),
        KEY_HOME => Some(PopoverKey::Slide(SlideKey::Lowest)),
        KEY_END => Some(PopoverKey::Slide(SlideKey::Highest)),
        _ => None,
    }
}

pub(super) fn stepped(steps: &[Effort], from: Option<Effort>, key: SlideKey) -> Option<Effort> {
    let last = steps.len().checked_sub(1)?;
    let at = from.and_then(|from| steps.iter().position(|step| *step == from));
    let to = match (key, at) {
        (SlideKey::Lowest, _) | (SlideKey::Lower | SlideKey::Higher, None) => 0,
        (SlideKey::Highest, _) => last,
        (SlideKey::Lower, Some(at)) => at.saturating_sub(1),
        (SlideKey::Higher, Some(at)) => (at + 1).min(last),
    };
    steps.get(to).copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Trigger {
    Hidden,
    Disabled,
    Enabled,
}

pub(super) const fn trigger(offers_effort: bool, steps: usize) -> Trigger {
    match (offers_effort, steps) {
        (false, _) => Trigger::Hidden,
        (true, 0 | 1) => Trigger::Disabled,
        (true, _) => Trigger::Enabled,
    }
}

fn as_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

pub(super) fn level_share(step: usize, steps: usize) -> f32 {
    match steps {
        0 => 0.0,
        1 => 1.0,
        _ => as_f32(step.min(steps - 1)) / as_f32(steps - 1),
    }
}

pub(super) fn thumb_left(step: usize, steps: usize) -> f32 {
    level_share(step, steps) * (TRACK_W - THUMB_W)
}

pub(super) fn fill_width(step: usize, steps: usize) -> f32 {
    thumb_left(step, steps) + THUMB_W
}

pub(super) fn dot_columns() -> usize {
    let mut columns = 0;
    while as_f32(columns + 1) * DOT_PITCH <= TRACK_W {
        columns += 1;
    }
    columns
}

fn scatter(column: usize, row: usize) -> f32 {
    let column = u32::try_from(column).unwrap_or_default();
    let row = u32::try_from(row).unwrap_or_default();
    let mixed = column.wrapping_mul(HASH_MIX_COL) ^ row.wrapping_mul(HASH_MIX_ROW);
    let mixed = mixed ^ (mixed >> 15);
    let bucket = u16::try_from(mixed % u32::from(HASH_BUCKETS)).unwrap_or_default();
    f32::from(bucket) / f32::from(HASH_BUCKETS)
}

pub(super) fn dot_alpha(column: usize, row: usize, columns: usize, strength: f32) -> Option<u32> {
    let along = level_share(column, columns.max(2));
    let glow = along * strength.clamp(0.0, 1.0);
    let density = DOT_SPREAD.mul_add(glow, DOT_FLOOR);
    if scatter(column, row) >= density {
        return None;
    }
    let levels = DOT_ALPHAS.len();
    let brighter = (1..levels)
        .filter(|level| glow > level_share(*level, levels))
        .count();
    DOT_ALPHAS.get(brighter).copied()
}

pub(super) fn level_tone(share: f32) -> f32 {
    (1.0 - MIN_TONE).mul_add(share.clamp(0.0, 1.0), MIN_TONE)
}

pub(super) fn mix(from: Rgba, to: Rgba, share: f32) -> Rgba {
    let share = share.clamp(0.0, 1.0);
    let blend = |from: f32, to: f32| (to - from).mul_add(share, from);
    Rgba {
        r: blend(from.r, to.r),
        g: blend(from.g, to.g),
        b: blend(from.b, to.b),
        a: blend(from.a, to.a),
    }
}

pub(super) fn snap_bounce(delta: f32) -> f32 {
    let t = delta.clamp(0.0, 1.0) - 1.0;
    let lift = OVERSHOOT + 1.0;
    (lift * t).mul_add(t * t, OVERSHOOT * t * t) + 1.0
}

pub(super) fn popover_left(anchor_x: f32, container_w: f32) -> f32 {
    let widest = (container_w - POPOVER_W - EDGE_MARGIN).max(EDGE_MARGIN);
    (anchor_x - POPOVER_W / 2.0).clamp(EDGE_MARGIN, widest)
}

#[cfg(test)]
mod tests;
