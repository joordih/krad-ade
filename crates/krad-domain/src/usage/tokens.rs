use serde::{Deserialize, Serialize};
use std::iter::Sum;
use std::ops::{Add, AddAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
}

impl TokenUsage {
    #[must_use]
    pub const fn total(self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.total() == 0
    }
}

impl Add for TokenUsage {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            input: self.input.saturating_add(other.input),
            output: self.output.saturating_add(other.output),
            cache_read: self.cache_read.saturating_add(other.cache_read),
            cache_write: self.cache_write.saturating_add(other.cache_write),
        }
    }
}

impl AddAssign for TokenUsage {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl Sum for TokenUsage {
    fn sum<I: Iterator<Item = Self>>(usages: I) -> Self {
        usages.fold(Self::default(), Add::add)
    }
}
