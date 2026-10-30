use krad_domain::{Checklist, ChecklistItem, ChecklistStatus};
use serde::Deserialize;
use serde_json::Value;

pub(crate) const PLAN_UPDATED: &str = "turn/plan/updated";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum StepStatus {
    InProgress,
    Completed,
    #[serde(other)]
    Pending,
}

impl From<StepStatus> for ChecklistStatus {
    fn from(status: StepStatus) -> Self {
        match status {
            StepStatus::Pending => Self::Pending,
            StepStatus::InProgress => Self::InProgress,
            StepStatus::Completed => Self::Done,
        }
    }
}

#[derive(Deserialize)]
struct PlanStep {
    step: String,
    status: StepStatus,
}

#[derive(Deserialize)]
struct PlanNotice {
    plan: Vec<PlanStep>,
}

pub(crate) fn checklist(params: &Value) -> Option<Checklist> {
    let PlanNotice { plan } = PlanNotice::deserialize(params).ok()?;
    Some(
        plan.into_iter()
            .map(|step| ChecklistItem {
                text: step.step,
                status: step.status.into(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_updated_codex_plan_becomes_the_checklist() {
        let list = checklist(
            &json!({"threadId": "t", "turnId": "turn-1", "explanation": null,
            "plan": [
                {"step": "Read the parser", "status": "completed"},
                {"step": "Fix the bug", "status": "inProgress"},
                {"step": "Run the tests", "status": "pending"}
            ]}),
        )
        .unwrap_or_default();
        let statuses: Vec<(&str, ChecklistStatus)> = list
            .items
            .iter()
            .map(|item| (item.text.as_str(), item.status))
            .collect();
        assert_eq!(
            statuses,
            [
                ("Read the parser", ChecklistStatus::Done),
                ("Fix the bug", ChecklistStatus::InProgress),
                ("Run the tests", ChecklistStatus::Pending),
            ]
        );
        assert_eq!(checklist(&json!({"threadId": "t"})), None);
    }
}
