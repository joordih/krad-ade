use std::f32::consts::TAU;

const MAX_SUBSTEP: f32 = 1.0 / 240.0;
const SETTLE_DISTANCE: f32 = 0.001;
const SETTLE_SPEED: f32 = 0.01;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Spring {
    value: f32,
    velocity: f32,
    target: f32,
    stiffness: f32,
    damping: f32,
    resting: bool,
}

impl Spring {
    pub(crate) fn at_rest(value: f32, response: f32) -> Self {
        let angular = TAU / response;
        Self {
            value,
            velocity: 0.0,
            target: value,
            stiffness: angular * angular,
            damping: 2.0 * angular,
            resting: true,
        }
    }

    pub(crate) const fn value(&self) -> f32 {
        self.value
    }

    pub(crate) fn set_target(&mut self, target: f32) {
        self.target = target;
        self.resting = false;
    }

    pub(crate) const fn is_settled(&self) -> bool {
        self.resting
    }

    pub(crate) fn step(&mut self, elapsed: f32) {
        let mut remaining = elapsed.max(0.0);
        while remaining > 0.0 {
            let dt = remaining.min(MAX_SUBSTEP);
            let force = -self.stiffness * (self.value - self.target) - self.damping * self.velocity;
            self.velocity += force * dt;
            self.value += self.velocity * dt;
            remaining -= dt;
        }
        if (self.value - self.target).abs() < SETTLE_DISTANCE && self.velocity.abs() < SETTLE_SPEED
        {
            self.value = self.target;
            self.velocity = 0.0;
            self.resting = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;
    const RESPONSE: f32 = 0.35;

    fn run(spring: &mut Spring, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|_| {
                spring.step(FRAME);
                spring.value()
            })
            .collect()
    }

    #[test]
    fn settles_on_target_without_overshoot() {
        let mut spring = Spring::at_rest(1.0, RESPONSE);
        spring.set_target(0.0);
        let path = run(&mut spring, 120);
        assert!(
            path.iter().all(|value| *value >= 0.0),
            "no overshoot past 0"
        );
        assert!(
            path.windows(2).all(|pair| pair[1] <= pair[0]),
            "moves one way"
        );
        assert!(spring.is_settled(), "rests within two seconds");
    }

    #[test]
    fn retargeting_mid_flight_continues_from_the_current_value() {
        let mut spring = Spring::at_rest(1.0, RESPONSE);
        spring.set_target(0.0);
        run(&mut spring, 6);
        let before = spring.value();
        spring.set_target(1.0);
        spring.step(FRAME);
        let jump = (spring.value() - before).abs();
        assert!(
            jump < 0.1,
            "reversal moves smoothly instead of jumping ({jump})"
        );
        run(&mut spring, 120);
        assert!(spring.is_settled());
        assert!((spring.value() - 1.0).abs() < f32::EPSILON);
    }
}
