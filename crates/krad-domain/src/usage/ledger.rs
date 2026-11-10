use super::prices::ModelPrices;
use super::report::{RunningSpend, Spend, UsageReport};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatUsage {
    #[serde(default, skip_serializing_if = "Spend::is_empty")]
    settled: Spend,
    #[serde(default, skip_serializing_if = "Spend::is_empty")]
    running: Spend,
}

impl ChatUsage {
    #[must_use]
    pub fn continuing(spent: Spend) -> Self {
        Self {
            settled: Spend::default(),
            running: spent,
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.settled.is_empty() && self.running.is_empty()
    }

    #[must_use]
    pub fn spent(&self) -> Spend {
        self.settled + self.running
    }

    pub fn record(&mut self, report: &UsageReport, prices: &ModelPrices) {
        match report.spend_after(self.running, prices) {
            RunningSpend::Grew(running) => self.running = running,
            RunningSpend::Restarted(running) => {
                self.settled += self.running;
                self.running = running;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelId;
    use crate::usage::{ReportedUsage, TokenUsage, UsdCost};

    fn dollars(amount: f64) -> UsdCost {
        UsdCost::try_from(amount).unwrap_or_default()
    }

    fn running(cost: f64, output: u64) -> UsageReport {
        UsageReport::RunningTotal(ReportedUsage {
            tokens: TokenUsage {
                output,
                ..TokenUsage::default()
            },
            cost: Some(dollars(cost)),
            model: None,
        })
    }

    fn added(model: &str, input: u64) -> UsageReport {
        UsageReport::Added(ReportedUsage {
            tokens: TokenUsage {
                input,
                ..TokenUsage::default()
            },
            cost: None,
            model: ModelId::parse(model),
        })
    }

    #[test]
    fn a_running_total_replaces_the_last_one_instead_of_adding_to_it() {
        let mut usage = ChatUsage::default();
        let prices = ModelPrices::bundled();
        usage.record(&running(0.10, 100), &prices);
        usage.record(&running(0.25, 300), &prices);
        assert_eq!(usage.spent().cost, dollars(0.25));
        assert_eq!(usage.spent().tokens.output, 300);
    }

    #[test]
    fn a_running_total_that_starts_over_keeps_what_was_spent_before() {
        let mut usage = ChatUsage::default();
        let prices = ModelPrices::bundled();
        usage.record(&running(0.40, 400), &prices);
        usage.record(&running(0.05, 50), &prices);
        usage.record(&running(0.10, 90), &prices);
        assert_eq!(usage.spent().cost, dollars(0.50));
        assert_eq!(usage.spent().tokens.output, 490);
    }

    #[test]
    fn added_usage_is_priced_by_its_model_and_summed() {
        let mut usage = ChatUsage::default();
        let prices = ModelPrices::bundled();
        usage.record(&added("claude-opus-5-5", 1_000_000), &prices);
        usage.record(&added("claude-opus-5-5", 500_000), &prices);
        assert_eq!(usage.spent().cost, dollars(6.0));
        assert_eq!(usage.spent().tokens.input, 1_500_000);
    }

    #[test]
    fn usage_of_a_model_without_a_price_counts_its_tokens_at_no_cost() {
        let mut usage = ChatUsage::default();
        usage.record(&added("llama-4", 1_000), &ModelPrices::bundled());
        assert_eq!(usage.spent().cost, UsdCost::ZERO);
        assert_eq!(usage.spent().tokens.input, 1_000);
    }

    #[test]
    fn a_resumed_chat_that_reports_its_whole_spend_again_is_not_counted_twice() {
        let earlier = Spend {
            tokens: TokenUsage::default(),
            cost: dollars(1.0),
        };
        let mut usage = ChatUsage::continuing(earlier);
        usage.record(&running(1.20, 0), &ModelPrices::bundled());
        assert_eq!(usage.spent().cost, dollars(1.20));
    }

    #[test]
    fn a_chat_saved_before_usage_was_kept_loads_with_none() {
        let usage: ChatUsage = serde_json::from_str("{}").unwrap_or_else(|_| ChatUsage {
            running: Spend {
                tokens: TokenUsage::default(),
                cost: dollars(9.0),
            },
            ..ChatUsage::default()
        });
        assert!(usage.is_empty());
    }
}
