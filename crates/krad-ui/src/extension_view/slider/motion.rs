use krad_extensions::api::view::{Enabled, Slider, ViewEvent};
use std::time::{Duration, Instant};

pub(super) const CHANGE_INTERVAL: Duration = Duration::from_millis(50);
const DEFAULT_KEY_STEPS: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Hold {
    Following,
    Dragging { value: f64, sent: Sent },
    Committed { value: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Sent {
    at: Instant,
    value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyMove {
    Back,
    Forward,
    Start,
    End,
}

pub(super) fn key_step(spec: &Slider) -> f64 {
    spec.step
        .filter(|step| *step > 0.0)
        .unwrap_or_else(|| (spec.max - spec.min) / DEFAULT_KEY_STEPS)
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SliderMotion {
    pub(super) spec: Slider,
    anchor: Instant,
    hold: Hold,
}

impl SliderMotion {
    pub(super) const fn new(spec: Slider, now: Instant) -> Self {
        Self {
            spec,
            anchor: now,
            hold: Hold::Following,
        }
    }

    pub(super) fn receive(&mut self, spec: Slider, now: Instant) {
        *self = Self::new(spec, now);
    }

    fn span(&self) -> f64 {
        self.spec.max - self.spec.min
    }

    fn bounded(&self, value: f64) -> f64 {
        if value.is_finite() && self.spec.min < self.spec.max {
            value.clamp(self.spec.min, self.spec.max)
        } else {
            self.spec.min
        }
    }

    fn snapped(&self, value: f64) -> f64 {
        let Some(step) = self.spec.step.filter(|step| *step > 0.0) else {
            return self.bounded(value);
        };
        let steps = ((value - self.spec.min) / step).round();
        self.bounded(steps.mul_add(step, self.spec.min))
    }

    pub(super) fn value_at(&self, now: Instant) -> f64 {
        match self.hold {
            Hold::Dragging { value, .. } | Hold::Committed { value } => value,
            Hold::Following => {
                let rate = self.spec.advancing.map_or(0.0, |advancing| advancing.rate);
                let elapsed = now.saturating_duration_since(self.anchor).as_secs_f64();
                self.bounded(rate.mul_add(elapsed, self.spec.value))
            }
        }
    }

    pub(super) fn fraction_at(&self, now: Instant) -> f64 {
        let span = self.span();
        if span > 0.0 {
            ((self.value_at(now) - self.spec.min) / span).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub(super) fn is_advancing(&self, now: Instant) -> bool {
        self.hold == Hold::Following
            && self.spec.advancing.is_some()
            && self.value_at(now) < self.spec.max
    }

    pub(super) const fn is_dragging(&self) -> bool {
        matches!(self.hold, Hold::Dragging { .. })
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.spec.enabled == Enabled::Yes
    }

    fn at_fraction(&self, fraction: f64) -> f64 {
        self.snapped(fraction.clamp(0.0, 1.0).mul_add(self.span(), self.spec.min))
    }

    fn change(&self, value: f64) -> ViewEvent {
        ViewEvent::SliderChange {
            action: self.spec.action.clone(),
            value,
        }
    }

    fn commit(&self, value: f64) -> ViewEvent {
        ViewEvent::SliderCommit {
            action: self.spec.action.clone(),
            value,
        }
    }

    pub(super) fn press(&mut self, fraction: f64, now: Instant) -> Option<ViewEvent> {
        if !self.is_enabled() {
            return None;
        }
        let value = self.at_fraction(fraction);
        self.hold = Hold::Dragging {
            value,
            sent: Sent { at: now, value },
        };
        Some(self.change(value))
    }

    pub(super) fn drag(&mut self, fraction: f64, now: Instant) -> Option<ViewEvent> {
        let Hold::Dragging { sent, .. } = self.hold else {
            return None;
        };
        let value = self.at_fraction(fraction);
        let due = now.saturating_duration_since(sent.at) >= CHANGE_INTERVAL;
        if !due || value.total_cmp(&sent.value).is_eq() {
            self.hold = Hold::Dragging { value, sent };
            return None;
        }
        self.hold = Hold::Dragging {
            value,
            sent: Sent { at: now, value },
        };
        Some(self.change(value))
    }

    pub(super) fn release(&mut self) -> Option<ViewEvent> {
        let Hold::Dragging { value, .. } = self.hold else {
            return None;
        };
        self.hold = Hold::Committed { value };
        Some(self.commit(value))
    }

    pub(super) fn key(&mut self, step: KeyMove, now: Instant) -> Option<ViewEvent> {
        if !self.is_enabled() || self.is_dragging() {
            return None;
        }
        let current = self.value_at(now);
        let value = match step {
            KeyMove::Back => self.snapped(current - key_step(&self.spec)),
            KeyMove::Forward => self.snapped(current + key_step(&self.spec)),
            KeyMove::Start => self.spec.min,
            KeyMove::End => self.spec.max,
        };
        self.hold = Hold::Committed { value };
        Some(self.commit(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::api::view::{Advancing, SliderFormat, SliderLabels};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn timeline(value: f64) -> Result<Slider, Box<dyn std::error::Error>> {
        Ok(Slider {
            action: "seek".parse()?,
            value,
            min: 0.0,
            max: 100.0,
            step: None,
            format: SliderFormat::Duration,
            labels: SliderLabels::ElapsedRemaining,
            enabled: Enabled::Yes,
            advancing: Some(Advancing { rate: 2.0 }),
        })
    }

    fn committed(event: Option<ViewEvent>) -> Option<f64> {
        match event? {
            ViewEvent::SliderCommit { value, .. } => Some(value),
            _ => None,
        }
    }

    fn changed(event: Option<ViewEvent>) -> Option<f64> {
        match event? {
            ViewEvent::SliderChange { value, .. } => Some(value),
            _ => None,
        }
    }

    #[test]
    fn an_advancing_slider_moves_from_the_patch_and_stops_at_max() -> TestResult {
        let start = Instant::now();
        let motion = SliderMotion::new(timeline(10.0)?, start);
        assert_eq!(motion.value_at(start), 10.0);
        assert_eq!(motion.value_at(start + Duration::from_secs(5)), 20.0);
        assert!(motion.is_advancing(start + Duration::from_secs(5)));
        assert_eq!(motion.value_at(start + Duration::from_secs(600)), 100.0);
        assert!(!motion.is_advancing(start + Duration::from_secs(600)));
        Ok(())
    }

    #[test]
    fn a_still_slider_never_asks_for_frames() -> TestResult {
        let start = Instant::now();
        let mut volume = timeline(40.0)?;
        volume.advancing = None;
        let motion = SliderMotion::new(volume, start);
        assert_eq!(motion.value_at(start + Duration::from_secs(9)), 40.0);
        assert!(!motion.is_advancing(start));
        Ok(())
    }

    #[test]
    fn dragging_pauses_advancing_until_the_program_patches_again() -> TestResult {
        let start = Instant::now();
        let mut motion = SliderMotion::new(timeline(10.0)?, start);
        assert_eq!(changed(motion.press(0.5, start)), Some(50.0));
        let later = start + Duration::from_secs(3);
        assert_eq!(motion.value_at(later), 50.0);
        assert!(!motion.is_advancing(later));
        assert_eq!(committed(motion.release()), Some(50.0));
        assert_eq!(motion.value_at(later + Duration::from_secs(3)), 50.0);
        motion.receive(timeline(50.0)?, later);
        assert_eq!(motion.value_at(later + Duration::from_secs(1)), 52.0);
        Ok(())
    }

    #[test]
    fn changes_while_dragging_are_throttled_and_the_release_commits_the_last() -> TestResult {
        let start = Instant::now();
        let mut motion = SliderMotion::new(timeline(0.0)?, start);
        let half = CHANGE_INTERVAL / 2;
        assert!(motion.press(0.1, start).is_some());
        assert_eq!(motion.drag(0.2, start + half), None);
        assert_eq!(
            changed(motion.drag(0.2, start + CHANGE_INTERVAL)),
            Some(20.0)
        );
        assert_eq!(motion.drag(0.45, start + CHANGE_INTERVAL + half), None);
        assert_eq!(committed(motion.release()), Some(45.0));
        assert_eq!(motion.release(), None);
        let mut still = SliderMotion::new(timeline(0.0)?, start);
        assert!(still.press(0.1, start).is_some());
        assert_eq!(still.drag(0.1, start + CHANGE_INTERVAL * 2), None);
        Ok(())
    }

    #[test]
    fn values_snap_to_the_step_and_stay_in_range() -> TestResult {
        let start = Instant::now();
        let mut stepped = timeline(0.0)?;
        stepped.step = Some(15.0);
        let mut motion = SliderMotion::new(stepped, start);
        assert_eq!(changed(motion.press(0.29, start)), Some(30.0));
        assert_eq!(
            changed(motion.drag(1.7, start + CHANGE_INTERVAL)),
            Some(100.0)
        );
        assert_eq!(
            changed(motion.drag(-1.0, start + CHANGE_INTERVAL * 2)),
            Some(0.0)
        );
        Ok(())
    }

    #[test]
    fn arrows_move_by_step_and_home_and_end_jump_to_the_ends() -> TestResult {
        let start = Instant::now();
        let mut volume = timeline(50.0)?;
        volume.advancing = None;
        volume.step = Some(10.0);
        let mut motion = SliderMotion::new(volume, start);
        assert_eq!(committed(motion.key(KeyMove::Forward, start)), Some(60.0));
        assert_eq!(committed(motion.key(KeyMove::Back, start)), Some(50.0));
        assert_eq!(committed(motion.key(KeyMove::End, start)), Some(100.0));
        assert_eq!(committed(motion.key(KeyMove::Forward, start)), Some(100.0));
        assert_eq!(committed(motion.key(KeyMove::Start, start)), Some(0.0));
        let mut unstepped = SliderMotion::new(timeline(50.0)?, start);
        assert_eq!(
            committed(unstepped.key(KeyMove::Forward, start)),
            Some(50.0 + 100.0 / DEFAULT_KEY_STEPS)
        );
        Ok(())
    }

    #[test]
    fn a_disabled_slider_ignores_the_pointer_and_keys_but_still_advances() -> TestResult {
        let start = Instant::now();
        let mut locked = timeline(10.0)?;
        locked.enabled = Enabled::No;
        let mut motion = SliderMotion::new(locked, start);
        assert_eq!(motion.press(0.5, start), None);
        assert_eq!(motion.key(KeyMove::End, start), None);
        assert_eq!(motion.value_at(start + Duration::from_secs(1)), 12.0);
        Ok(())
    }
}
