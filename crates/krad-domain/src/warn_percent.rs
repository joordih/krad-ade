use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

const FULL_PERCENT: u16 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct WarnPercent(u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidWarnPercent;

impl fmt::Display for InvalidWarnPercent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a warning comes at a percent from 1 to 99")
    }
}

impl std::error::Error for InvalidWarnPercent {}

impl WarnPercent {
    pub(crate) const fn known(percent: u8) -> Self {
        Self(percent)
    }

    #[must_use]
    pub fn reached_by(self, used: u16) -> bool {
        used >= u16::from(self.0)
    }

    #[must_use]
    pub fn reached_by_share(self, used: u128, whole: u128) -> bool {
        used * u128::from(FULL_PERCENT) >= whole * u128::from(self.0)
    }
}

impl TryFrom<u8> for WarnPercent {
    type Error = InvalidWarnPercent;

    fn try_from(percent: u8) -> Result<Self, Self::Error> {
        if percent == 0 || u16::from(percent) >= FULL_PERCENT {
            return Err(InvalidWarnPercent);
        }
        Ok(Self(percent))
    }
}

impl From<WarnPercent> for u8 {
    fn from(warning: WarnPercent) -> Self {
        warning.0
    }
}

pub(crate) fn lenient_warn_percent<'de, D: Deserializer<'de>>(
    deserializer: D,
    fallback: WarnPercent,
) -> Result<WarnPercent, D::Error> {
    let stored = serde_json::Value::deserialize(deserializer)?;
    Ok(WarnPercent::deserialize(stored).unwrap_or(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_warning_percent_must_fall_inside_the_limit() {
        assert!(WarnPercent::try_from(0).is_err());
        assert!(WarnPercent::try_from(100).is_err());
        assert_eq!(WarnPercent::try_from(90).map(u8::from), Ok(90));
    }

    #[test]
    fn a_warning_is_reached_at_its_percent() {
        let warning = WarnPercent::known(80);
        assert!(!warning.reached_by(79));
        assert!(warning.reached_by(80));
        assert!(warning.reached_by_share(8, 10));
        assert!(!warning.reached_by_share(7, 10));
    }
}
