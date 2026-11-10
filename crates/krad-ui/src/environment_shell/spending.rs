use gpui_kit::Context;
use krad_domain::{
    BudgetStatus, EnvironmentData, EnvironmentName, GroupId, ModelId, Spend, TerminalId,
    UsageReport, UsdCost,
};

use super::EnvironmentShell;

const UNGROUPED_NAME: &str = "Terminals";
const CAPTION_SEPARATOR: &str = " · ";
const BUDGET_SEPARATOR: &str = " / ";
const TOKENS_SUFFIX: &str = "tokens";
const THOUSAND_SUFFIX: &str = "k";
const MILLION_SUFFIX: &str = "M";
const THOUSAND: u64 = 1_000;
const MILLION: u64 = 1_000_000;
const TENTHS: u64 = 10;

fn scaled(tokens: u64, unit: u64, suffix: &str) -> String {
    let tenths = tokens.saturating_mul(TENTHS) / unit;
    let (whole, tenth) = (tenths / TENTHS, tenths % TENTHS);
    if tenth == 0 {
        return format!("{whole}{suffix}");
    }
    format!("{whole}.{tenth}{suffix}")
}

pub(super) fn token_count(tokens: u64) -> String {
    if tokens < THOUSAND {
        return tokens.to_string();
    }
    if tokens < MILLION {
        return scaled(tokens, THOUSAND, THOUSAND_SUFFIX);
    }
    scaled(tokens, MILLION, MILLION_SUFFIX)
}

pub(super) fn spend_caption(spend: &Spend) -> Option<String> {
    (!spend.is_empty()).then(|| {
        format!(
            "{}{CAPTION_SEPARATOR}{} {TOKENS_SUFFIX}",
            spend.cost,
            token_count(spend.tokens.total())
        )
    })
}

pub(super) fn budget_caption(spent: UsdCost, budget: Option<UsdCost>) -> String {
    match budget.filter(|budget| !budget.is_zero()) {
        Some(budget) => format!("{spent}{BUDGET_SEPARATOR}{budget}"),
        None => spent.to_string(),
    }
}

fn group_name(data: &EnvironmentData, group: Option<GroupId>) -> String {
    group
        .and_then(|group| data.group(group))
        .map_or_else(|| UNGROUPED_NAME.to_owned(), |group| group.name.clone())
}

pub(super) fn budget_warning(
    group: &str,
    status: BudgetStatus,
    spent: UsdCost,
    budget: UsdCost,
) -> Option<String> {
    match status {
        BudgetStatus::Within => None,
        BudgetStatus::Approaching => {
            Some(format!("{group} has spent {spent} of its {budget} budget"))
        }
        BudgetStatus::Exceeded => Some(format!(
            "{group} is over its {budget} budget: {spent} spent"
        )),
    }
}

pub(super) struct GroupSpendLabel {
    pub(super) text: String,
    pub(super) status: Option<BudgetStatus>,
}

impl EnvironmentShell {
    pub(super) fn record_spend(
        &mut self,
        env: &EnvironmentName,
        chat: TerminalId,
        report: UsageReport,
        model: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let prices = self.settings.spending.model_prices();
        let report = report.or_model(model.and_then(ModelId::parse));
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let group = live.data.group_of_chat(chat);
        let before = live.data.group_spend(group).cost;
        live.data.record_usage(chat, &report, &prices);
        let after = live.data.group_spend(group).cost;
        self.note_variant_spend(env, chat);
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let spending = &self.settings.spending;
        let crossed = spending
            .status(before)
            .zip(spending.status(after))
            .and_then(|(before, after)| BudgetStatus::crossed(before, after));
        let warning = crossed
            .zip(spending.group_budget)
            .and_then(|(status, budget)| {
                budget_warning(&group_name(&live.data, group), status, after, budget)
            });
        if let Some(warning) = warning {
            self.push_toast(warning);
        }
        cx.notify();
        self.store();
    }

    pub(super) fn group_spend_label(&self, group: Option<GroupId>) -> Option<GroupSpendLabel> {
        let live = self.envs.get(&self.active)?;
        let spent = live.data.group_spend(group).cost;
        let budget = self.settings.spending.group_budget;
        if spent.is_zero() {
            return None;
        }
        Some(GroupSpendLabel {
            text: budget_caption(spent, budget),
            status: self.settings.spending.status(spent),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dollars(amount: f64) -> UsdCost {
        UsdCost::try_from(amount).unwrap_or_default()
    }

    #[test]
    fn token_counts_shorten_to_thousands_and_millions() {
        assert_eq!(token_count(950), "950");
        assert_eq!(token_count(1_000), "1k");
        assert_eq!(token_count(12_345), "12.3k");
        assert_eq!(token_count(2_500_000), "2.5M");
    }

    #[test]
    fn a_session_that_spent_nothing_shows_no_caption() {
        assert_eq!(spend_caption(&Spend::default()), None);
        let spent = Spend {
            tokens: krad_domain::TokenUsage {
                input: 12_000,
                output: 345,
                ..krad_domain::TokenUsage::default()
            },
            cost: dollars(0.42),
        };
        assert_eq!(
            spend_caption(&spent).as_deref(),
            Some("$0.42 · 12.3k tokens")
        );
    }

    #[test]
    fn a_group_shows_its_spend_against_the_budget_when_there_is_one() {
        assert_eq!(budget_caption(dollars(1.5), None), "$1.50");
        assert_eq!(
            budget_caption(dollars(1.5), Some(dollars(20.0))),
            "$1.50 / $20.00"
        );
    }

    #[test]
    fn the_warning_says_how_close_the_group_is_to_its_budget() {
        let budget = dollars(20.0);
        assert_eq!(
            budget_warning("api", BudgetStatus::Within, dollars(1.0), budget),
            None
        );
        assert_eq!(
            budget_warning("api", BudgetStatus::Approaching, dollars(16.0), budget).as_deref(),
            Some("api has spent $16.00 of its $20.00 budget")
        );
        assert_eq!(
            budget_warning("api", BudgetStatus::Exceeded, dollars(21.0), budget).as_deref(),
            Some("api is over its $20.00 budget: $21.00 spent")
        );
    }
}
