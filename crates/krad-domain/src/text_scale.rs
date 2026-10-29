use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct TextScale(f32);

impl TextScale {
    pub const MIN: f32 = 0.6;
    pub const MAX: f32 = 2.5;
    pub const DEFAULT: f32 = 1.0;
    pub const STEP: f32 = 1.1;

    #[must_use]
    pub fn new(raw: f32) -> Self {
        Self(raw.clamp(Self::MIN, Self::MAX))
    }

    #[must_use]
    pub fn bigger(self) -> Self {
        Self::new(self.0 * Self::STEP)
    }

    #[must_use]
    pub fn smaller(self) -> Self {
        Self::new(self.0 / Self::STEP)
    }

    #[must_use]
    pub const fn as_f32(self) -> f32 {
        self.0
    }
}

impl Default for TextScale {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_close;

    #[test]
    fn text_scale_steps_and_stays_within_bounds() {
        let scale = TextScale::default().bigger().bigger();
        assert_close(scale.as_f32(), TextScale::STEP * TextScale::STEP);
        assert_close(scale.smaller().smaller().as_f32(), TextScale::DEFAULT);
        assert_close(TextScale::new(9.0).as_f32(), TextScale::MAX);
        let mut tiny = TextScale::default();
        for _ in 0..50 {
            tiny = tiny.smaller();
        }
        assert_close(tiny.as_f32(), TextScale::MIN);
    }
}
