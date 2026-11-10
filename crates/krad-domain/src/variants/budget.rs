use crate::usage::{BudgetStatus, SpendSettings, UsdCost};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchEstimate {
    pub variants: usize,
    pub per_variant: Option<UsdCost>,
}

impl LaunchEstimate {
    #[must_use]
    pub fn total(self) -> Option<UsdCost> {
        let count = u64::try_from(self.variants).unwrap_or(u64::MAX);
        self.per_variant
            .map(|cost| UsdCost::from_nanos(cost.nanos().saturating_mul(count)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchBudgetWarning {
    pub status: BudgetStatus,
    pub spent: UsdCost,
    pub budget: UsdCost,
    pub variants: usize,
    pub estimated: Option<UsdCost>,
}

#[must_use]
pub fn launch_budget_warning(
    settings: &SpendSettings,
    spent: UsdCost,
    estimate: LaunchEstimate,
) -> Option<LaunchBudgetWarning> {
    let budget = settings.group_budget.filter(|budget| !budget.is_zero())?;
    let estimated = estimate.total();
    let projected = spent + estimated.unwrap_or_default();
    let status = settings.status(projected)?;
    (status != BudgetStatus::Within).then_some(LaunchBudgetWarning {
        status,
        spent,
        budget,
        variants: estimate.variants,
        estimated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::warn_percent::WarnPercent;

    fn dollars(amount: f64) -> UsdCost {
        UsdCost::try_from(amount).unwrap_or_default()
    }

    fn budget(limit: f64) -> SpendSettings {
        SpendSettings {
            group_budget: Some(dollars(limit)),
            warn_at: WarnPercent::known(80),
            prices: Vec::new(),
        }
    }

    fn estimate(variants: usize, per_variant: Option<f64>) -> LaunchEstimate {
        LaunchEstimate {
            variants,
            per_variant: per_variant.map(dollars),
        }
    }

    #[test]
    fn without_a_budget_a_launch_never_warns() {
        let settings = SpendSettings::default();
        let warning = launch_budget_warning(&settings, dollars(500.0), estimate(4, Some(9.0)));
        assert_eq!(warning, None);
    }

    #[test]
    fn a_launch_warns_when_the_estimate_would_take_the_group_near_its_budget() {
        let settings = budget(10.0);
        let quiet = launch_budget_warning(&settings, dollars(2.0), estimate(4, Some(1.0)));
        assert_eq!(quiet, None);
        let near = launch_budget_warning(&settings, dollars(2.0), estimate(4, Some(1.5)));
        assert_eq!(
            near,
            Some(LaunchBudgetWarning {
                status: BudgetStatus::Approaching,
                spent: dollars(2.0),
                budget: dollars(10.0),
                variants: 4,
                estimated: Some(dollars(6.0)),
            })
        );
        let over = launch_budget_warning(&settings, dollars(2.0), estimate(4, Some(2.0)));
        assert_eq!(
            over.map(|warning| warning.status),
            Some(BudgetStatus::Exceeded)
        );
    }

    #[test]
    fn without_an_estimate_only_the_spend_so_far_decides() {
        let settings = budget(10.0);
        assert_eq!(
            launch_budget_warning(&settings, dollars(7.0), estimate(3, None)),
            None
        );
        let near = launch_budget_warning(&settings, dollars(8.5), estimate(3, None));
        assert_eq!(
            near.map(|warning| (warning.status, warning.estimated)),
            Some((BudgetStatus::Approaching, None))
        );
    }
}
