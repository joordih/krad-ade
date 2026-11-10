use super::cost::UsdCost;
use super::prices::ModelPrices;
use super::tokens::TokenUsage;
use crate::model::ModelId;
use serde::{Deserialize, Serialize};
use std::iter::Sum;
use std::ops::{Add, AddAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Spend {
    #[serde(default)]
    pub tokens: TokenUsage,
    #[serde(default)]
    pub cost: UsdCost,
}

impl Spend {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.cost.is_zero()
    }

    const fn shrank_from(&self, before: &Self) -> bool {
        self.cost.nanos() < before.cost.nanos() || self.tokens.total() < before.tokens.total()
    }

    #[must_use]
    pub fn less(self, other: Self) -> Self {
        Self {
            tokens: TokenUsage {
                input: self.tokens.input.saturating_sub(other.tokens.input),
                output: self.tokens.output.saturating_sub(other.tokens.output),
                cache_read: self
                    .tokens
                    .cache_read
                    .saturating_sub(other.tokens.cache_read),
                cache_write: self
                    .tokens
                    .cache_write
                    .saturating_sub(other.tokens.cache_write),
            },
            cost: UsdCost::from_nanos(self.cost.nanos().saturating_sub(other.cost.nanos())),
        }
    }
}

impl Add for Spend {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            tokens: self.tokens + other.tokens,
            cost: self.cost + other.cost,
        }
    }
}

impl AddAssign for Spend {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl Sum for Spend {
    fn sum<I: Iterator<Item = Self>>(spends: I) -> Self {
        spends.fold(Self::default(), Add::add)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportedUsage {
    pub tokens: TokenUsage,
    pub cost: Option<UsdCost>,
    pub model: Option<ModelId>,
}

impl ReportedUsage {
    #[must_use]
    pub fn priced(&self, prices: &ModelPrices) -> Spend {
        let by_model = || {
            let model = self.model.as_ref()?;
            prices.cost(model.as_str(), self.tokens)
        };
        Spend {
            tokens: self.tokens,
            cost: self.cost.or_else(by_model).unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageReport {
    Added(ReportedUsage),
    RunningTotal(ReportedUsage),
}

impl UsageReport {
    #[must_use]
    pub fn or_model(mut self, model: Option<ModelId>) -> Self {
        let (Self::Added(usage) | Self::RunningTotal(usage)) = &mut self;
        if usage.model.is_none() {
            usage.model = model;
        }
        self
    }

    pub(super) fn spend_after(&self, before: Spend, prices: &ModelPrices) -> RunningSpend {
        match self {
            Self::Added(usage) => RunningSpend::Grew(before + usage.priced(prices)),
            Self::RunningTotal(usage) => {
                let total = usage.priced(prices);
                if total.shrank_from(&before) {
                    RunningSpend::Restarted(total)
                } else {
                    RunningSpend::Grew(total)
                }
            }
        }
    }
}

pub(super) enum RunningSpend {
    Grew(Spend),
    Restarted(Spend),
}
