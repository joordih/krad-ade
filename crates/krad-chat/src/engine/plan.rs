use krad_domain::PermissionMode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;

use super::wire::ToolSpec;

pub(crate) const PLAN_TOOL: &str = "exit_plan";
pub(crate) const PLAN_LABEL: &str = "ExitPlanMode";
const PLAN_DESCRIPTION: &str = "Present your plan to the user and ask to leave Plan mode. Call it only in Plan mode, once you have explored the code read-only and the plan is ready. The plan is Markdown: the steps, the files to change and how to verify the result. If the user approves, the same turn continues in the mode they picked and you implement the plan; if not, keep planning with their feedback.";
const BAD_PLAN_TEXT: &str = "Invalid arguments for exit_plan:";
const EMPTY_PLAN_TEXT: &str = "exit_plan needs a plan";
pub(crate) const NOT_IN_PLAN_TEXT: &str =
    "This chat is not in Plan mode, so there is nothing to exit; just proceed with the task.";
const APPROVED_TEXT: &str = "The user approved the plan; continue and implement it in";
const APPROVED_TAIL: &str = "mode.";
const REJECTED_TEXT: &str = "The user rejected the plan and the chat stays in Plan mode. Keep exploring read-only and call exit_plan again with a revised plan.";
const FEEDBACK_LABEL: &str = "Feedback:";
const HEADING_MARKS: &[char] = &['#', ' '];

#[derive(Deserialize)]
struct PlanArgs {
    plan: String,
}

pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: Cow::Borrowed(PLAN_TOOL),
        description: Cow::Borrowed(PLAN_DESCRIPTION),
        parameters: json!({
            "type": "object",
            "properties": {
                "plan": {"type": "string", "description": "The plan, in Markdown"}
            },
            "required": ["plan"]
        }),
    }
}

pub(crate) fn parse(arguments: &Value) -> Result<String, String> {
    let PlanArgs { plan } =
        PlanArgs::deserialize(arguments).map_err(|error| format!("{BAD_PLAN_TEXT} {error}"))?;
    if plan.trim().is_empty() {
        return Err(EMPTY_PLAN_TEXT.to_owned());
    }
    Ok(plan)
}

pub(crate) fn summary(plan: &str) -> String {
    plan.lines()
        .map(|line| line.trim_start_matches(HEADING_MARKS).trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned()
}

pub(crate) fn approved(mode: PermissionMode) -> String {
    format!("{APPROVED_TEXT} {mode} {APPROVED_TAIL}")
}

pub(crate) fn rejected(message: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        return REJECTED_TEXT.to_owned();
    }
    format!("{REJECTED_TEXT} {FEEDBACK_LABEL} {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_parses_and_shows_its_first_line() {
        let plan = parse(&json!({"plan": "\n# Fix the parser\n\n1. Read it"})).unwrap_or_default();
        assert_eq!(summary(&plan), "Fix the parser");
        assert!(parse(&json!({"plan": "  "})).is_err());
        assert!(parse(&json!({})).is_err());
    }

    #[test]
    fn answers_tell_the_model_how_to_go_on() {
        assert_eq!(
            approved(PermissionMode::AcceptEdits),
            "The user approved the plan; continue and implement it in acceptEdits mode."
        );
        assert!(rejected("smaller steps").ends_with("Feedback: smaller steps"));
        assert_eq!(rejected(" "), REJECTED_TEXT);
    }
}
