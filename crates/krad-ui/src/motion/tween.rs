use std::time::{Duration, Instant};

use super::easing::Easing;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Tween {
    from: f32,
    to: f32,
    flight: Option<Flight>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Flight {
    started: Instant,
    duration: Duration,
    easing: Easing,
}

impl Flight {
    fn share(self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started);
        (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }
}

impl Tween {
    #[must_use]
    pub(crate) const fn at_rest(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            flight: None,
        }
    }

    #[must_use]
    pub(crate) fn value(&self, now: Instant) -> f32 {
        let Some(flight) = self.flight else {
            return self.to;
        };
        let eased = flight.easing.apply(flight.share(now));
        self.from + (self.to - self.from) * eased
    }

    pub(crate) fn retarget(&mut self, to: f32, duration: Duration, easing: Easing, now: Instant) {
        if (to - self.to).abs() < f32::EPSILON {
            return;
        }
        self.from = self.value(now);
        self.to = to;
        self.flight = Some(Flight {
            started: now,
            duration,
            easing,
        });
    }

    pub(crate) fn advance(&mut self, now: Instant) -> bool {
        let Some(flight) = self.flight else {
            return false;
        };
        if flight.share(now) >= 1.0 {
            self.settle();
        }
        self.flight.is_some()
    }

    pub(crate) fn settle(&mut self) {
        self.from = self.to;
        self.flight = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPAN: Duration = Duration::from_millis(200);
    const HALF: Duration = Duration::from_millis(100);

    #[test]
    fn a_resting_tween_reports_its_value_and_does_not_move() {
        let mut tween = Tween::at_rest(0.5);
        let now = Instant::now();
        assert!(!tween.advance(now));
        assert!((tween.value(now) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn a_tween_eases_toward_its_target_and_comes_to_rest() {
        let start = Instant::now();
        let mut tween = Tween::at_rest(0.0);
        tween.retarget(1.0, SPAN, Easing::OutCubic, start);
        assert!(tween.value(start).abs() < f32::EPSILON);
        let halfway = tween.value(start + HALF);
        assert!(halfway > 0.5 && halfway < 1.0, "{halfway}");
        assert!(tween.advance(start + HALF));
        assert!(!tween.advance(start + SPAN));
        assert!((tween.value(start + SPAN) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn retargeting_mid_flight_starts_from_where_it_is_shown() {
        let start = Instant::now();
        let mut tween = Tween::at_rest(0.0);
        tween.retarget(1.0, SPAN, Easing::Emphasized, start);
        let shown = tween.value(start + HALF);
        tween.retarget(0.0, SPAN, Easing::Emphasized, start + HALF);
        assert!((tween.value(start + HALF) - shown).abs() < f32::EPSILON);
        assert!(!tween.advance(start + HALF + SPAN));
        assert!(tween.value(start + HALF + SPAN).abs() < f32::EPSILON);
    }

    #[test]
    fn retargeting_to_the_same_target_keeps_the_flight() {
        let start = Instant::now();
        let mut tween = Tween::at_rest(0.0);
        tween.retarget(1.0, SPAN, Easing::OutCubic, start);
        tween.retarget(1.0, SPAN, Easing::OutCubic, start + HALF);
        assert!(!tween.advance(start + SPAN));
    }

    #[test]
    fn settling_jumps_to_the_target() {
        let start = Instant::now();
        let mut tween = Tween::at_rest(0.0);
        tween.retarget(1.0, SPAN, Easing::OutCubic, start);
        tween.settle();
        assert!(!tween.advance(start));
        assert!((tween.value(start) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_zero_duration_lands_at_once() {
        let start = Instant::now();
        let mut tween = Tween::at_rest(0.0);
        tween.retarget(1.0, Duration::ZERO, Easing::OutCubic, start);
        assert!((tween.value(start) - 1.0).abs() < f32::EPSILON);
        assert!(!tween.advance(start));
    }
}
