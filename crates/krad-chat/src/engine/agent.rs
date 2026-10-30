use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;

use super::checklist::CHECKLIST_TOOL;
use super::plan::PLAN_TOOL;
use super::questions::QUESTIONS_TOOL;
use super::wire::ToolSpec;

pub(crate) const AGENT_TOOL: &str = "agent";
pub(crate) const AGENT_LABEL: &str = "Agent";
pub(crate) const MAX_SUBAGENTS: usize = 4;
const AGENT_DESCRIPTION: &str = "Start a subagent: a helper with a fresh context, your tools and your permission mode, that works on one task on its own and answers with a report. Use it for broad searches, research or independent pieces of work that would fill your context; several agent calls in one reply run in parallel. The subagent sees only the prompt, so make it complete: the goal, what you already know, the files involved and what the report must contain. Its report is the result of this call; the user does not see it unless you pass it on.";
const SUBAGENT_BRIEF: &str = "## Subagent\nYou are a subagent started by another agent for one task. You cannot ask the user questions, keep a checklist, start other agents or leave Plan mode. Work on your own until the task is done, then answer with a concise report of what you found or changed, with paths and names; that report is all the agent that started you sees.";
const BAD_AGENT_TEXT: &str = "Invalid arguments for agent:";
const EMPTY_PROMPT_TEXT: &str = "agent needs a prompt";
const NO_REPORT_TEXT: &str = "The subagent ended without a report.";
const FAILED_TEXT: &str = "The subagent failed:";
const SECTION_GAP: &str = "\n\n";
const PARENT_ONLY: [&str; 4] = [AGENT_TOOL, QUESTIONS_TOOL, PLAN_TOOL, CHECKLIST_TOOL];

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct Task {
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) prompt: String,
}

pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: Cow::Borrowed(AGENT_TOOL),
        description: Cow::Borrowed(AGENT_DESCRIPTION),
        parameters: json!({
            "type": "object",
            "properties": {
                "description": {"type": "string", "description": "The task in three to five words"},
                "prompt": {"type": "string", "description": "The full task for the subagent"}
            },
            "required": ["description", "prompt"]
        }),
    }
}

pub(crate) fn parse(arguments: &Value) -> Result<Task, String> {
    let task = Task::deserialize(arguments).map_err(|error| format!("{BAD_AGENT_TEXT} {error}"))?;
    if task.prompt.trim().is_empty() {
        return Err(EMPTY_PROMPT_TEXT.to_owned());
    }
    Ok(task)
}

pub(crate) fn system(parent: &str) -> String {
    format!("{parent}{SECTION_GAP}{SUBAGENT_BRIEF}")
}

pub(crate) fn tools(parent: &[ToolSpec]) -> Vec<ToolSpec> {
    parent
        .iter()
        .filter(|spec| !PARENT_ONLY.contains(&spec.name.as_ref()))
        .cloned()
        .collect()
}

pub(crate) fn report(answer: Option<String>, failure: Option<String>) -> (String, bool) {
    match (answer, failure) {
        (Some(answer), _) => (answer, false),
        (None, Some(failure)) => (format!("{FAILED_TEXT} {failure}"), true),
        (None, None) => (NO_REPORT_TEXT.to_owned(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{checklist, plan, questions};

    #[test]
    fn an_agent_call_needs_a_prompt_and_shows_its_description() {
        let arguments = json!({"description": "Map the UI", "prompt": "Find every view"});
        assert_eq!(
            parse(&arguments).map(|task| task.prompt),
            Ok("Find every view".to_owned())
        );
        assert!(parse(&json!({"description": "x", "prompt": " "})).is_err());
        assert!(parse(&json!({"description": "x"})).is_err());
    }

    #[test]
    fn subagents_keep_the_tools_but_not_the_ones_that_talk_to_the_user() {
        let parent: Vec<ToolSpec> = crate::engine::tools::specs()
            .into_iter()
            .chain([questions::spec(), plan::spec(), checklist::spec(), spec()])
            .collect();
        let kept = tools(&parent);
        let dropped: Vec<&str> = parent
            .iter()
            .filter(|spec| !kept.contains(spec))
            .map(|spec| spec.name.as_ref())
            .collect();
        assert_eq!(dropped, ["ask_user", "exit_plan", "checklist", "agent"]);
        assert!(system("Base").ends_with(SUBAGENT_BRIEF));
    }

    #[test]
    fn the_report_is_the_answer_or_why_there_is_none() {
        assert_eq!(
            report(Some("Found it".to_owned()), None),
            ("Found it".to_owned(), false)
        );
        assert_eq!(
            report(None, Some("timeout".to_owned())),
            ("The subagent failed: timeout".to_owned(), true)
        );
        assert!(report(None, None).1);
    }
}
