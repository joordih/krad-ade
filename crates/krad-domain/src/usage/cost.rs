use serde::{Deserialize, Serialize};
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign};

const NANOS_PER_DOLLAR: u32 = 1_000_000_000;
const NANOS_PER_CENT: u64 = 10_000_000;
const CENTS_PER_DOLLAR: u64 = 100;
const DOLLAR_SIGN: &str = "$";
const BELOW_A_CENT: &str = "<$0.01";

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct UsdCost(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCost;

impl fmt::Display for InvalidCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a cost is a finite amount of dollars, zero or more")
    }
}

impl std::error::Error for InvalidCost {}

impl UsdCost {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    #[must_use]
    pub const fn nanos(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub fn dollars(self) -> f64 {
        let per_dollar = u64::from(NANOS_PER_DOLLAR);
        let whole = u32::try_from(self.0 / per_dollar).unwrap_or(u32::MAX);
        let rest = u32::try_from(self.0 % per_dollar).unwrap_or_default();
        f64::from(whole) + f64::from(rest) / f64::from(NANOS_PER_DOLLAR)
    }
}

impl TryFrom<f64> for UsdCost {
    type Error = InvalidCost;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn try_from(dollars: f64) -> Result<Self, Self::Error> {
        let nanos = (dollars * f64::from(NANOS_PER_DOLLAR)).round();
        if !nanos.is_finite() || nanos < 0.0 {
            return Err(InvalidCost);
        }
        Ok(Self(nanos as u64))
    }
}

impl From<UsdCost> for f64 {
    fn from(cost: UsdCost) -> Self {
        cost.dollars()
    }
}

impl Add for UsdCost {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl AddAssign for UsdCost {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl Sum for UsdCost {
    fn sum<I: Iterator<Item = Self>>(costs: I) -> Self {
        costs.fold(Self::ZERO, Add::add)
    }
}

impl fmt::Display for UsdCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cents = self.0.saturating_add(NANOS_PER_CENT / 2) / NANOS_PER_CENT;
        if cents == 0 && self.0 > 0 {
            return f.write_str(BELOW_A_CENT);
        }
        let whole = cents / CENTS_PER_DOLLAR;
        let rest = cents % CENTS_PER_DOLLAR;
        write!(f, "{DOLLAR_SIGN}{whole}.{rest:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cost_parses_from_dollars_and_rejects_what_is_not_an_amount() {
        assert_eq!(UsdCost::try_from(0.000_339_12), Ok(UsdCost(339_120)));
        assert_eq!(UsdCost::try_from(-1.0), Err(InvalidCost));
        assert_eq!(UsdCost::try_from(f64::NAN), Err(InvalidCost));
        assert_eq!(UsdCost::try_from(f64::INFINITY), Err(InvalidCost));
    }

    #[test]
    fn a_cost_shows_dollars_and_cents_and_marks_what_is_below_a_cent() {
        assert_eq!(UsdCost::ZERO.to_string(), "$0.00");
        assert_eq!(UsdCost(339_120).to_string(), "<$0.01");
        assert_eq!(UsdCost(1_234_000_000).to_string(), "$1.23");
        assert_eq!(UsdCost(12_995_000_000).to_string(), "$13.00");
    }

    #[test]
    fn a_cost_turns_back_into_the_dollars_it_came_from() {
        let cost = UsdCost::try_from(2.5).unwrap_or_default();
        assert!((cost.dollars() - 2.5).abs() < f64::EPSILON);
    }
}
