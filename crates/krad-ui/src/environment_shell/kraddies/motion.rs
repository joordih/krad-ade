use gpui_kit::{
    px, Animation, AnimationElement, AnimationExt as _, AnyElement, ElementId, IntoElement,
    ParentElement, SharedString, SpringConfig, Styled,
};
use std::f32::consts::TAU;
use std::time::Duration;

pub(super) use crate::components::motion::{ease_out_back, ease_out_cubic};
use crate::components::motion::{MOTION_HIDDEN, MOTION_SETTLED};

pub(super) const ENTER: Duration = Duration::from_millis(320);
pub(super) const STAGGER: Duration = Duration::from_millis(45);
pub(super) const MAX_STAGGERED: usize = 8;
pub(super) const RISE: f32 = 10.0;
pub(super) const FADE: Duration = Duration::from_millis(140);
pub(super) const COUNT_UP: Duration = Duration::from_millis(700);
pub(super) const COUNT_STEPS: usize = 14;
pub(super) const HEAT_WAVE: Duration = Duration::from_millis(900);
pub(super) const HEAT_WAVE_SPREAD: f32 = 6.0;
pub(super) const MOOD_POP: Duration = Duration::from_millis(520);
pub(super) const MOOD_POP_FROM: f32 = 0.82;
pub(super) const WIGGLE: Duration = Duration::from_millis(560);
pub(super) const WIGGLE_SWAY: f32 = 4.0;
pub(super) const WIGGLE_TURNS: f32 = 3.0;
pub(super) const IDLE_BOB: Duration = Duration::from_millis(2600);
pub(super) const IDLE_BOB_LIFT: f32 = 3.0;
pub(super) const IDLE_MAX_FPS: f32 = 30.0;
pub(super) const SHAKE: Duration = Duration::from_millis(380);
pub(super) const SHAKE_SWAY: f32 = 6.0;
pub(super) const SHAKE_TURNS: f32 = 4.0;
pub(super) const SAVED_HOLD: Duration = Duration::from_millis(1600);
pub(super) const SLIDE_SPRING: SpringConfig = crate::theme::MOTION_SPRING;

#[must_use]
pub(super) fn decaying_sway(t: f32, sway: f32, turns: f32) -> f32 {
    let t = t.clamp(MOTION_HIDDEN, MOTION_SETTLED);
    (t * turns * TAU).sin() * sway * (1.0 - t)
}

#[must_use]
pub(super) fn bob(t: f32) -> f32 {
    -(t * TAU).sin().abs() * IDLE_BOB_LIFT
}

#[must_use]
pub(super) fn stagger_delay(index: usize) -> Duration {
    let steps = u32::try_from(index.min(MAX_STAGGERED)).unwrap_or_default();
    STAGGER * steps
}

#[must_use]
pub(super) fn wave_reveal(progress: f32, column: usize, columns: usize) -> f32 {
    let columns = u16::try_from(columns.max(1)).map_or(f32::from(u16::MAX), f32::from);
    let column = u16::try_from(column).map_or(columns, f32::from);
    let front = progress.clamp(MOTION_HIDDEN, MOTION_SETTLED) * (columns + HEAT_WAVE_SPREAD);
    ((front - column) / HEAT_WAVE_SPREAD).clamp(MOTION_HIDDEN, MOTION_SETTLED)
}

#[must_use]
pub(super) fn count_steps(value: u64) -> Vec<u64> {
    let last = u16::try_from(COUNT_STEPS.max(1)).map_or(f32::from(u16::MAX), f32::from);
    (1..=COUNT_STEPS)
        .map(|step| {
            let step = u16::try_from(step).map_or(last, f32::from);
            let shown = f64::from(ease_out_cubic(step / last));
            scale_count(value, shown)
        })
        .collect()
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn scale_count(value: u64, shown: f64) -> u64 {
    ((value as f64) * shown).round().clamp(0.0, value as f64) as u64
}

#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub(super) fn count_frame(steps: usize, t: f32) -> usize {
    let last = steps.saturating_sub(1);
    ((t.clamp(MOTION_HIDDEN, MOTION_SETTLED) * last as f32).round() as usize).min(last)
}

pub(super) fn idling(avatar: impl IntoElement, id: impl Into<ElementId>) -> AnyElement {
    gpui_kit::div()
        .relative()
        .child(avatar)
        .with_animation(
            id,
            Animation::new(IDLE_BOB).repeat().with_max_fps(IDLE_MAX_FPS),
            |avatar, t| avatar.top(px(bob(t))),
        )
        .into_any_element()
}

pub(super) fn entering<E: IntoElement + Styled + 'static>(
    element: E,
    id: impl Into<ElementId>,
    index: usize,
) -> AnimationElement<E> {
    let delay = stagger_delay(index);
    let rise = |element: E, shown: f32| {
        let shown = ease_out_cubic(shown);
        element
            .relative()
            .opacity(shown)
            .top(px(RISE * (MOTION_SETTLED - shown)))
    };
    if delay.is_zero() {
        return element.with_animations(id, vec![Animation::new(ENTER)], move |element, _, t| {
            rise(element, t)
        });
    }
    element.with_animations(
        id,
        vec![Animation::new(delay), Animation::new(ENTER)],
        move |element, phase, t| rise(element, if phase == 0 { MOTION_HIDDEN } else { t }),
    )
}

pub(super) fn fading_in<E: IntoElement + Styled + 'static>(
    element: E,
    id: impl Into<ElementId>,
) -> AnimationElement<E> {
    element.with_animation(id, Animation::new(FADE), |element, t| {
        element.opacity(ease_out_cubic(t))
    })
}

pub(super) fn counting<E: IntoElement + gpui_kit::ParentElement + 'static>(
    element: E,
    id: impl Into<ElementId>,
    frames: Vec<SharedString>,
) -> AnimationElement<E> {
    element.with_animation(id, Animation::new(COUNT_UP), move |element, t| {
        let shown = frames.get(count_frame(frames.len(), t)).cloned();
        element.children(shown)
    })
}

pub(super) fn shaking<E: IntoElement + Styled + 'static>(
    element: E,
    id: impl Into<ElementId>,
) -> AnimationElement<E> {
    element.with_animation(id, Animation::new(SHAKE), |element, t| {
        element
            .relative()
            .left(px(decaying_sway(t, SHAKE_SWAY, SHAKE_TURNS)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-4;

    #[test]
    fn sways_die_out_and_the_bob_only_lifts() {
        assert!(decaying_sway(1.0, WIGGLE_SWAY, WIGGLE_TURNS).abs() < EPSILON);
        assert!(decaying_sway(0.0, SHAKE_SWAY, SHAKE_TURNS).abs() < EPSILON);
        assert!((0_u8..=40).all(|step| {
            let lift = bob(f32::from(step) / 40.0);
            (-IDLE_BOB_LIFT..=0.0).contains(&lift)
        }));
    }

    #[test]
    fn staggering_is_capped_so_long_lists_still_arrive_quickly() {
        assert_eq!(stagger_delay(0), Duration::ZERO);
        assert_eq!(stagger_delay(2), STAGGER * 2);
        assert_eq!(stagger_delay(500), stagger_delay(MAX_STAGGERED));
        assert!(stagger_delay(MAX_STAGGERED) + ENTER < Duration::from_secs(1));
    }

    #[test]
    fn the_heat_wave_sweeps_from_the_oldest_week_to_the_newest() {
        assert!(wave_reveal(0.0, 0, 27).abs() < EPSILON);
        assert!((wave_reveal(1.0, 26, 27) - 1.0).abs() < EPSILON);
        assert!(wave_reveal(0.3, 0, 27) > wave_reveal(0.3, 20, 27));
        assert!((wave_reveal(1.0, 0, 27) - 1.0).abs() < EPSILON);
    }

    #[test]
    fn counting_up_climbs_without_passing_the_value() {
        let steps = count_steps(250);
        assert_eq!(steps.len(), COUNT_STEPS);
        assert_eq!(steps.last(), Some(&250));
        assert!(steps.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(count_steps(0).iter().all(|step| *step == 0));
        assert_eq!(count_frame(COUNT_STEPS, 0.0), 0);
        assert_eq!(count_frame(COUNT_STEPS, 1.0), COUNT_STEPS - 1);
        assert_eq!(count_frame(0, 0.5), 0);
    }
}
