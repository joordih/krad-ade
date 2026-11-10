use super::cost::UsdCost;
use super::prices::{ModelPrice, ModelPrices};
use crate::warn_percent::{lenient_warn_percent, WarnPercent};
use serde::{Deserialize, Deserializer, Serialize};

const DEFAULT_BUDGET_WARNING: WarnPercent = WarnPercent::known(80);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BudgetStatus {
    Within,
    Approaching,
    Exceeded,
}

impl BudgetStatus {
    #[must_use]
    pub fn crossed(before: Self, after: Self) -> Option<Self> {
        (after > before).then_some(after)
    }
}

const fn default_budget_warning() -> WarnPercent {
    DEFAULT_BUDGET_WARNING
}

fn lenient_warning<'de, D: Deserializer<'de>>(deserializer: D) -> Result<WarnPercent, D::Error> {
    lenient_warn_percent(deserializer, DEFAULT_BUDGET_WARNING)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_budget: Option<UsdCost>,
    #[serde(
        default = "default_budget_warning",
        deserialize_with = "lenient_warning"
    )]
    pub warn_at: WarnPercent,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prices: Vec<ModelPrice>,
}

impl Default for SpendSettings {
    fn default() -> Self {
        Self {
            group_budget: None,
            warn_at: DEFAULT_BUDGET_WARNING,
            prices: Vec::new(),
        }
    }
}

impl SpendSettings {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub fn model_prices(&self) -> ModelPrices {
        ModelPrices::with_overrides(&self.prices)
    }

    #[must_use]
    pub fn status(&self, spent: UsdCost) -> Option<BudgetStatus> {
        let budget = self.group_budget.filter(|budget| !budget.is_zero())?;
        let spent = u128::from(spent.nanos());
        let budget = u128::from(budget.nanos());
        if spent >= budget {
            return Some(BudgetStatus::Exceeded);
        }
        Some(if self.warn_at.reached_by_share(spent, budget) {
            BudgetStatus::Approaching
        } else {
            BudgetStatus::Within
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dollars(amount: f64) -> UsdCost {
        UsdCost::try_from(amount).unwrap_or_default()
    }

    fn budget(limit: f64, percent: u8) -> SpendSettings {
        SpendSettings {
            group_budget: Some(dollars(limit)),
            warn_at: WarnPercent::try_from(percent).unwrap_or(DEFAULT_BUDGET_WARNING),
            prices: Vec::new(),
        }
    }

    #[test]
    fn without_a_budget_there_is_nothing_to_warn_about() {
        assert_eq!(SpendSettings::default().status(dollars(1_000.0)), None);
        assert_eq!(budget(0.0, 80).status(dollars(1.0)), None);
    }

    #[test]
    fn spend_is_within_then_approaching_then_over_the_budget() {
        let settings = budget(10.0, 80);
        assert_eq!(settings.status(dollars(7.99)), Some(BudgetStatus::Within));
        assert_eq!(
            settings.status(dollars(8.0)),
            Some(BudgetStatus::Approaching)
        );
        assert_eq!(
            settings.status(dollars(9.99)),
            Some(BudgetStatus::Approaching)
        );
        assert_eq!(settings.status(dollars(10.0)), Some(BudgetStatus::Exceeded));
    }

    #[test]
    fn a_warning_comes_only_when_spend_crosses_into_a_worse_status() {
        use BudgetStatus::{Approaching, Exceeded, Within};
        assert_eq!(
            BudgetStatus::crossed(Within, Approaching),
            Some(Approaching)
        );
        assert_eq!(BudgetStatus::crossed(Approaching, Exceeded), Some(Exceeded));
        assert_eq!(BudgetStatus::crossed(Within, Exceeded), Some(Exceeded));
        assert_eq!(BudgetStatus::crossed(Approaching, Approaching), None);
        assert_eq!(BudgetStatus::crossed(Exceeded, Exceeded), None);
        assert_eq!(BudgetStatus::crossed(Approaching, Within), None);
    }

    #[test]
    fn a_warning_percent_outside_the_budget_falls_back_to_the_default() {
        let stored: Result<SpendSettings, _> = serde_json::from_str(r#"{"warn_at":150}"#);
        assert_eq!(
            stored.ok().map(|stored| stored.warn_at),
            Some(DEFAULT_BUDGET_WARNING)
        );
        let older: Result<SpendSettings, _> = serde_json::from_str("{}");
        assert_eq!(older.ok(), Some(SpendSettings::default()));
    }
}
