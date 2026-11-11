use std::time::Duration;

pub(crate) const THEME_FADE: Duration = Duration::from_millis(360);
pub(crate) const LAYOUT_SHIFT: Duration = Duration::from_millis(220);

const EMPHASIZED: CubicBezier = CubicBezier::new(0.16, 1.0, 0.3, 1.0);
const NEWTON_STEPS: usize = 8;
const BISECTION_STEPS: usize = 24;
const SLOPE_FLOOR: f32 = 1e-6;
const SOLVE_TOLERANCE: f32 = 1e-5;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Easing {
    #[default]
    OutCubic,
    Emphasized,
}

impl Easing {
    #[must_use]
    pub(crate) fn apply(self, t: f32) -> f32 {
        match self {
            Self::OutCubic => ease_out_cubic(t),
            Self::Emphasized => ease_out_emphasized(t),
        }
    }
}

#[must_use]
pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

#[must_use]
pub(crate) fn ease_out_emphasized(t: f32) -> f32 {
    EMPHASIZED.ease(t)
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct CubicBezier {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl CubicBezier {
    const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn ease(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        if x <= 0.0 || x >= 1.0 {
            return x;
        }
        Self::curve(self.y1, self.y2, self.solve(x))
    }

    fn curve(first: f32, second: f32, t: f32) -> f32 {
        let rest = 1.0 - t;
        3.0 * rest * rest * t * first + 3.0 * rest * t * t * second + t * t * t
    }

    fn slope(first: f32, second: f32, t: f32) -> f32 {
        let rest = 1.0 - t;
        3.0 * rest * rest * first + 6.0 * rest * t * (second - first) + 3.0 * t * t * (1.0 - second)
    }

    fn solve(self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..NEWTON_STEPS {
            let error = Self::curve(self.x1, self.x2, t) - x;
            if error.abs() < SOLVE_TOLERANCE {
                return t;
            }
            let slope = Self::slope(self.x1, self.x2, t);
            if slope.abs() < SLOPE_FLOOR {
                break;
            }
            t = (t - error / slope).clamp(0.0, 1.0);
        }
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        t = x;
        for _ in 0..BISECTION_STEPS {
            let found = Self::curve(self.x1, self.x2, t);
            if (found - x).abs() < SOLVE_TOLERANCE {
                break;
            }
            if found < x {
                low = t;
            } else {
                high = t;
            }
            t = f32::midpoint(low, high);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: u16 = 100;

    fn samples() -> impl Iterator<Item = f32> {
        (0..=SAMPLES).map(|step| f32::from(step) / f32::from(SAMPLES))
    }

    #[test]
    fn every_curve_starts_at_rest_and_lands_on_the_target() {
        for easing in [Easing::OutCubic, Easing::Emphasized] {
            assert!(easing.apply(0.0).abs() < 1e-4, "{easing:?} at 0");
            assert!((easing.apply(1.0) - 1.0).abs() < 1e-4, "{easing:?} at 1");
            assert!(easing.apply(-1.0).abs() < 1e-4, "{easing:?} below 0");
            assert!((easing.apply(2.0) - 1.0).abs() < 1e-4, "{easing:?} past 1");
        }
    }

    #[test]
    fn every_curve_only_moves_forward_and_decelerates() {
        for easing in [Easing::OutCubic, Easing::Emphasized] {
            let path: Vec<f32> = samples().map(|t| easing.apply(t)).collect();
            assert!(
                path.windows(2).all(|pair| pair[1] + 1e-4 >= pair[0]),
                "{easing:?} never moves back"
            );
            assert!(easing.apply(0.5) > 0.5, "{easing:?} is an ease-out");
        }
    }

    #[test]
    fn ease_out_cubic_follows_its_formula() {
        assert!((ease_out_cubic(0.5) - 0.875).abs() < 1e-6);
        assert!((ease_out_cubic(0.25) - (1.0 - 0.75_f32.powi(3))).abs() < 1e-6);
    }

    #[test]
    fn the_emphasized_curve_matches_its_css_reference_points() {
        assert!((ease_out_emphasized(0.5) - 0.9718).abs() < 2e-3);
        assert!(ease_out_emphasized(0.1) > 0.4);
        assert!(ease_out_emphasized(0.9) > 0.99);
    }
}
