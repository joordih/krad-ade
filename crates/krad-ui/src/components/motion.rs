use gpui_kit::{
    px, Animation, AnimationExt as _, AnyElement, ElementId, IntoElement, SpringAnimation,
    SpringConfig, Styled,
};
use std::f32::consts::TAU;
use std::time::Duration;

pub(crate) use crate::motion::ease_out_cubic;

const MOTION_QUICK: Duration = Duration::from_millis(120);
pub(crate) const MOTION_BASE: Duration = Duration::from_millis(220);
const MOTION_GENTLE: Duration = Duration::from_millis(340);
pub(crate) const STAGGER_STEP: Duration = Duration::from_millis(32);
pub(crate) const STAGGER_LIMIT: usize = 12;
pub(crate) const PULSE_PERIOD: Duration = Duration::from_millis(1600);
pub(crate) const PULSE_FPS: f32 = 30.0;
pub(crate) const SHAKE_DURATION: Duration = Duration::from_millis(420);
pub(crate) const COUNT_UP_DURATION: Duration = Duration::from_millis(520);
pub(crate) const RISE_DISTANCE: f32 = 6.0;
pub(crate) const DROP_DISTANCE: f32 = 4.0;
pub(crate) const SHAKE_DISTANCE: f32 = 5.0;
pub(crate) const SHAKE_WAVES: f32 = 3.0;
pub(crate) const IDLE_BOB: f32 = 1.0;
const IDLE_BOB_FRAMES: u64 = 16;
pub(crate) const MOTION_GLIDE: SpringConfig = SpringConfig::new(380.0, 34.0, 1.0);
pub(crate) const MOTION_SETTLED: f32 = 1.0;
pub(crate) const MOTION_HIDDEN: f32 = 0.0;
const BACK_OVERSHOOT: f32 = 1.701_58;

#[must_use]
pub(crate) fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    let lift = BACK_OVERSHOOT + 1.0;
    1.0 + lift * t.powi(3) + BACK_OVERSHOOT * t.powi(2)
}

#[must_use]
pub(crate) fn stagger_delay(index: usize) -> Duration {
    let steps = u32::try_from(index.min(STAGGER_LIMIT)).unwrap_or_default();
    STAGGER_STEP * steps
}

#[must_use]
fn staggered_total(index: usize) -> Duration {
    MOTION_BASE + stagger_delay(index)
}

#[must_use]
pub(crate) fn staggered_progress(overall: f32, index: usize) -> f32 {
    let total = staggered_total(index).as_secs_f32();
    let delay = stagger_delay(index).as_secs_f32();
    let local = (overall * total - delay) / MOTION_BASE.as_secs_f32();
    ease_out_cubic(local)
}

#[must_use]
pub(crate) fn shake_offset(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (t * SHAKE_WAVES * TAU).sin() * (1.0 - t) * SHAKE_DISTANCE
}

#[must_use]
pub(crate) fn counted(t: f32, target: usize) -> usize {
    let Ok(whole) = u16::try_from(target) else {
        return target;
    };
    let shown = (ease_out_cubic(t) * f32::from(whole)).round();
    (0..whole)
        .find(|step| f32::from(*step) >= shown)
        .map_or(target, usize::from)
}

#[must_use]
pub(crate) fn idle_bob(frame: u64) -> f32 {
    let phase = f32::from(u16::try_from(frame % IDLE_BOB_FRAMES).unwrap_or_default())
        / f32::from(u16::try_from(IDLE_BOB_FRAMES).unwrap_or(1));
    (phase * TAU).sin() * IDLE_BOB
}

pub(crate) fn rise_in<E>(id: impl Into<ElementId>, index: usize, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    element
        .with_animation(
            id,
            Animation::new(staggered_total(index)),
            move |element, overall| {
                let shown = staggered_progress(overall, index);
                element
                    .opacity(shown)
                    .top(px(RISE_DISTANCE * (MOTION_SETTLED - shown)))
            },
        )
        .into_any_element()
}

pub(crate) fn drop_in<E>(id: impl Into<ElementId>, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    element
        .with_animation(
            id,
            Animation::new(MOTION_QUICK).with_easing(ease_out_cubic),
            |element, shown| {
                element
                    .opacity(shown)
                    .top(px(-DROP_DISTANCE * (MOTION_SETTLED - shown)))
            },
        )
        .into_any_element()
}

pub(crate) fn slide_from_side<E>(id: impl Into<ElementId>, distance: f32, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    element
        .with_animation(
            id,
            Animation::new(MOTION_BASE).with_easing(ease_out_cubic),
            move |element, shown| {
                element
                    .opacity(shown)
                    .left(px(distance * (MOTION_SETTLED - shown)))
            },
        )
        .into_any_element()
}

pub(crate) fn fade_in<E>(id: impl Into<ElementId>, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    element
        .with_animation(id, Animation::new(MOTION_QUICK), |element, shown| {
            element.opacity(shown)
        })
        .into_any_element()
}

pub(crate) fn shaking<E>(id: impl Into<ElementId>, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    element
        .with_animation(id, Animation::new(SHAKE_DURATION), |element, t| {
            element.left(px(shake_offset(t)))
        })
        .into_any_element()
}

pub(crate) fn pop<E>(id: impl Into<ElementId>, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    element
        .with_animation(id, Animation::new(MOTION_GENTLE), |element, t| {
            let lift = ease_out_back(t);
            element
                .opacity(ease_out_cubic(t))
                .top(px(RISE_DISTANCE * (MOTION_SETTLED - lift)))
        })
        .into_any_element()
}

pub(crate) fn reveal<E>(
    id: impl Into<ElementId>,
    open: bool,
    max_height: f32,
    element: E,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    let target = if open { MOTION_SETTLED } else { MOTION_HIDDEN };
    element
        .with_spring(
            id,
            SpringAnimation::new(MOTION_GLIDE).to(target),
            move |element, shown| {
                let shown = shown.clamp(MOTION_HIDDEN, MOTION_SETTLED);
                if open && shown >= MOTION_SETTLED {
                    return element;
                }
                element
                    .max_h(px(max_height * shown))
                    .opacity(shown)
                    .overflow_hidden()
            },
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_start_at_rest_and_settle_on_the_target() {
        assert!(ease_out_cubic(0.0).abs() < f32::EPSILON);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < f32::EPSILON);
        assert!(ease_out_cubic(0.5) > 0.5);
        assert!(ease_out_back(0.0).abs() < 1e-5);
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-5);
        assert!(ease_out_back(0.7) > 1.0);
    }

    #[test]
    fn later_items_wait_their_turn_and_the_wait_is_bounded() {
        assert_eq!(stagger_delay(0), Duration::ZERO);
        assert_eq!(stagger_delay(2), STAGGER_STEP * 2);
        assert_eq!(stagger_delay(500), stagger_delay(STAGGER_LIMIT));
        assert!(staggered_progress(0.0, 3).abs() < f32::EPSILON);
        assert!(staggered_progress(0.1, 6).abs() < f32::EPSILON);
        assert!((staggered_progress(1.0, 6) - 1.0).abs() < f32::EPSILON);
        assert!(staggered_progress(0.5, 0) > staggered_progress(0.5, 4));
    }

    #[test]
    fn a_shake_fades_out_and_ends_in_place() {
        assert!(shake_offset(0.0).abs() < f32::EPSILON);
        assert!(shake_offset(1.0).abs() < f32::EPSILON);
        let widest = (1..20)
            .map(|step| shake_offset(f32::from(u8::try_from(step).unwrap_or(0)) / 20.0).abs())
            .fold(0.0_f32, f32::max);
        assert!(widest <= SHAKE_DISTANCE);
        assert!(widest > SHAKE_DISTANCE / 2.0);
    }

    #[test]
    fn counting_up_lands_exactly_on_the_number() {
        assert_eq!(counted(0.0, 7), 0);
        assert_eq!(counted(1.0, 7), 7);
        assert!(counted(0.5, 40) < 40);
        assert_eq!(counted(1.0, 0), 0);
    }

    #[test]
    fn the_idle_bob_stays_within_a_pixel() {
        assert!((0..IDLE_BOB_FRAMES * 2).all(|frame| idle_bob(frame).abs() <= IDLE_BOB));
        assert!(idle_bob(0).abs() < f32::EPSILON);
    }
}
