use krad_domain::{Checklist, ChecklistItem};
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;

use super::wire::ToolSpec;

pub(crate) const CHECKLIST_TOOL: &str = "checklist";
const CHECKLIST_DESCRIPTION: &str = "Keep a checklist of the steps of the current task, shown to the user above the chat's composer. Use it for tasks with three or more steps: write the steps when you start, then call it again whenever a step starts or finishes. Each call replaces the whole list. Keep exactly one item in_progress while you work and mark items done as soon as they are finished.";
const BAD_CHECKLIST_TEXT: &str = "Invalid arguments for checklist:";
const EMPTY_CHECKLIST_TEXT: &str = "checklist needs at least one item with text";
const RECORDED_TEXT: &str = "Checklist updated:";
const DONE_OF: &str = "of";
const DONE_TAIL: &str = "done.";

#[derive(Deserialize)]
struct ChecklistArgs {
    items: Vec<ChecklistItem>,
}

pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: Cow::Borrowed(CHECKLIST_TOOL),
        description: Cow::Borrowed(CHECKLIST_DESCRIPTION),
        parameters: json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": {"type": "string", "description": "The step, in a few words"},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "done"]}
                        },
                        "required": ["text", "status"]
                    }
                }
            },
            "required": ["items"]
        }),
    }
}

pub(crate) fn parse(arguments: &Value) -> Result<Checklist, String> {
    let ChecklistArgs { items } = ChecklistArgs::deserialize(arguments)
        .map_err(|error| format!("{BAD_CHECKLIST_TEXT} {error}"))?;
    let list: Checklist = items.into_iter().collect();
    if list.items.is_empty() {
        return Err(EMPTY_CHECKLIST_TEXT.to_owned());
    }
    Ok(list)
}

pub(crate) fn summary(list: &Checklist) -> String {
    list.current()
        .map_or_else(String::new, |item| item.text.clone())
}

pub(crate) fn recorded(list: &Checklist) -> String {
    format!(
        "{RECORDED_TEXT} {} {DONE_OF} {} {DONE_TAIL}",
        list.done(),
        list.items.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ChecklistStatus;

    #[test]
    fn a_checklist_call_replaces_the_list_and_confirms_it() {
        let list = parse(&json!({"items": [
            {"text": "Read the parser", "status": "done"},
            {"text": "Fix the bug", "status": "in_progress"},
            {"text": "Run the tests", "status": "pending"}
        ]}))
        .unwrap_or_default();
        assert_eq!(list.items[1].status, ChecklistStatus::InProgress);
        assert_eq!(summary(&list), "Fix the bug");
        assert_eq!(recorded(&list), "Checklist updated: 1 of 3 done.");
    }

    #[test]
    fn a_checklist_needs_items_with_a_known_status() {
        assert!(parse(&json!({"items": []})).is_err());
        assert!(parse(&json!({"items": [{"text": " ", "status": "done"}]})).is_err());
        assert!(parse(&json!({"items": [{"text": "x", "status": "completed"}]})).is_err());
    }
}
