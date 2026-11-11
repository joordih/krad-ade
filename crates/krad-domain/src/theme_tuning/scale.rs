use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(from = "u16", into = "u16")]
pub struct InterfaceScale(u16);

impl InterfaceScale {
    pub const MIN: u16 = 80;
    pub const MAX: u16 = 150;
    pub const STEP: u16 = 10;
    pub const DEFAULT: u16 = 100;
    const WHOLE: f32 = 100.0;

    #[must_use]
    pub const fn new(percent: u16) -> Self {
        let clamped = if percent < Self::MIN {
            Self::MIN
        } else if percent > Self::MAX {
            Self::MAX
        } else {
            percent
        };
        let above = clamped - Self::MIN;
        let snapped = (above + Self::STEP / 2) / Self::STEP * Self::STEP;
        Self(Self::MIN + snapped)
    }

    #[must_use]
    pub const fn percent(self) -> u16 {
        self.0
    }

    #[must_use]
    pub fn factor(self) -> f32 {
        f32::from(self.0) / Self::WHOLE
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn steps() -> impl Iterator<Item = Self> {
        (Self::MIN..=Self::MAX)
            .step_by(usize::from(Self::STEP))
            .map(Self)
    }
}

impl Default for InterfaceScale {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl From<u16> for InterfaceScale {
    fn from(percent: u16) -> Self {
        Self::new(percent)
    }
}

impl From<InterfaceScale> for u16 {
    fn from(scale: InterfaceScale) -> Self {
        scale.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_stays_within_its_bounds_and_snaps_to_steps() {
        assert_eq!(InterfaceScale::new(10).percent(), InterfaceScale::MIN);
        assert_eq!(InterfaceScale::new(900).percent(), InterfaceScale::MAX);
        assert_eq!(InterfaceScale::new(114).percent(), 110);
        assert_eq!(InterfaceScale::new(115).percent(), 120);
        assert_eq!(InterfaceScale::default().percent(), 100);
        assert!((InterfaceScale::new(150).factor() - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn the_steps_run_from_the_minimum_to_the_maximum() {
        let steps: Vec<u16> = InterfaceScale::steps()
            .map(InterfaceScale::percent)
            .collect();
        assert_eq!(steps, [80, 90, 100, 110, 120, 130, 140, 150]);
        assert!(steps.contains(&InterfaceScale::DEFAULT));
    }

    #[test]
    fn a_stored_scale_is_read_back_within_bounds() -> serde_json::Result<()> {
        let read: InterfaceScale = serde_json::from_str("400")?;
        assert_eq!(read.percent(), InterfaceScale::MAX);
        assert_eq!(serde_json::to_string(&InterfaceScale::new(130))?, "130");
        Ok(())
    }
}
