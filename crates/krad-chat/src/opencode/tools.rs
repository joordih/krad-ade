use krad_domain::PermissionMode;
use serde::Serialize;

pub(crate) const PLAN_AGENT: &str = "plan";
pub(crate) const BUILD_AGENT: &str = "build";
const ANY: &str = "*";

const HARNESS_TOOL_NAMES: [(&str, &str); 13] = [
    ("shell", "Bash"),
    ("read", "Read"),
    ("edit", "Edit"),
    ("write", "Write"),
    ("patch", "Edit"),
    ("grep", "Grep"),
    ("glob", "Glob"),
    ("webfetch", "WebFetch"),
    ("websearch", "WebSearch"),
    ("subagent", "Task"),
    ("todowrite", "TodoWrite"),
    ("skill", "Skill"),
    ("question", "AskUserQuestion"),
];

const ASKED_IN_MANUAL: [&str; 4] = ["edit", "shell", "webfetch", "external_directory"];
const ASKED_ACCEPTING_EDITS: [&str; 3] = ["shell", "webfetch", "external_directory"];

pub(crate) fn harness_tool_name(tool: &str) -> &str {
    HARNESS_TOOL_NAMES
        .iter()
        .find(|(opencode, _)| *opencode == tool)
        .map_or(tool, |(_, shown)| shown)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Effect {
    Allow,
    Ask,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Rule {
    action: &'static str,
    resource: &'static str,
    effect: Effect,
}

const fn rule(action: &'static str, effect: Effect) -> Rule {
    Rule {
        action,
        resource: ANY,
        effect,
    }
}

pub(crate) fn rules(mode: PermissionMode) -> Vec<Rule> {
    let asked: &[&'static str] = match mode {
        PermissionMode::Manual | PermissionMode::Plan => &ASKED_IN_MANUAL,
        PermissionMode::AcceptEdits => &ASKED_ACCEPTING_EDITS,
        PermissionMode::Auto | PermissionMode::DontAsk | PermissionMode::BypassPermissions => &[],
    };
    std::iter::once(rule(ANY, Effect::Allow))
        .chain(asked.iter().map(|action| rule(action, Effect::Ask)))
        .collect()
}

pub(crate) const fn agent(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Plan => PLAN_AGENT,
        PermissionMode::Manual
        | PermissionMode::AcceptEdits
        | PermissionMode::Auto
        | PermissionMode::DontAsk
        | PermissionMode::BypassPermissions => BUILD_AGENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_tools_show_under_the_names_the_chat_draws() {
        assert_eq!(harness_tool_name("shell"), "Bash");
        assert_eq!(harness_tool_name("subagent"), "Task");
        assert_eq!(
            harness_tool_name("github_create_issue"),
            "github_create_issue"
        );
    }

    #[test]
    fn manual_mode_asks_before_edits_and_commands_and_bypass_asks_nothing() {
        let manual = serde_json::to_value(rules(PermissionMode::Manual))
            .unwrap_or_else(|_| unreachable!("the fixture is valid"));
        assert_eq!(manual[0]["effect"], "allow");
        assert_eq!(
            manual[1],
            serde_json::json!({"action":"edit","resource":"*","effect":"ask"})
        );
        assert_eq!(rules(PermissionMode::BypassPermissions).len(), 1);
        assert!(!rules(PermissionMode::AcceptEdits)
            .iter()
            .any(|rule| rule.action == "edit"));
        assert_eq!(agent(PermissionMode::Plan), PLAN_AGENT);
    }
}
