use krad_domain::{ReportedUsage, TokenUsage, UsageReport, UsdCost};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

const RESULT_TYPE: &str = "result";

#[derive(Default, Deserialize)]
struct ModelSpend {
    #[serde(default, rename = "inputTokens")]
    input: u64,
    #[serde(default, rename = "outputTokens")]
    output: u64,
    #[serde(default, rename = "cacheReadInputTokens")]
    cache_read: u64,
    #[serde(default, rename = "cacheCreationInputTokens")]
    cache_write: u64,
}

impl From<&ModelSpend> for TokenUsage {
    fn from(spend: &ModelSpend) -> Self {
        Self {
            input: spend.input,
            output: spend.output,
            cache_read: spend.cache_read,
            cache_write: spend.cache_write,
        }
    }
}

#[derive(Deserialize)]
struct ResultSpend {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    parent_tool_use_id: Option<String>,
    #[serde(default)]
    total_cost_usd: Option<f64>,
    #[serde(default, rename = "modelUsage")]
    model_usage: HashMap<String, ModelSpend>,
}

pub(crate) fn claude_spend(value: &Value) -> Option<UsageReport> {
    let result = ResultSpend::deserialize(value).ok()?;
    if result.kind != RESULT_TYPE || result.parent_tool_use_id.is_some() {
        return None;
    }
    let tokens: TokenUsage = result.model_usage.values().map(TokenUsage::from).sum();
    let cost = result
        .total_cost_usd
        .and_then(|cost| UsdCost::try_from(cost).ok());
    if tokens.is_empty() && cost.is_none_or(UsdCost::is_zero) {
        return None;
    }
    Some(UsageReport::RunningTotal(ReportedUsage {
        tokens,
        cost,
        model: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spend(line: &str) -> Option<UsageReport> {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|value| claude_spend(&value))
    }

    #[test]
    fn a_result_reports_the_running_total_of_the_conversation() {
        let recorded = include_str!("../../tests/fixtures/claude-recorded.jsonl");
        let reports: Vec<_> = recorded.lines().filter_map(spend).collect();
        assert_eq!(
            reports,
            [UsageReport::RunningTotal(ReportedUsage {
                tokens: TokenUsage {
                    input: 2,
                    output: 4,
                    cache_read: 33_692,
                    cache_write: 0,
                },
                cost: UsdCost::try_from(0.000_339_12).ok(),
                model: None,
            })]
        );
    }

    #[test]
    fn a_failed_turn_still_reports_what_it_spent() {
        let failed = spend(
            r#"{"type":"result","subtype":"error_max_turns","is_error":true,"total_cost_usd":0.5,"modelUsage":{}}"#,
        );
        assert!(matches!(
            failed,
            Some(UsageReport::RunningTotal(ReportedUsage { cost: Some(cost), .. }))
                if cost == UsdCost::try_from(0.5).unwrap_or_default()
        ));
    }

    #[test]
    fn other_lines_and_results_without_spend_report_nothing() {
        assert_eq!(spend(r#"{"type":"assistant","message":{}}"#), None);
        assert_eq!(spend(r#"{"type":"result","total_cost_usd":0}"#), None);
        assert_eq!(
            spend(r#"{"type":"result","parent_tool_use_id":"t1","total_cost_usd":1.0}"#),
            None
        );
    }
}
