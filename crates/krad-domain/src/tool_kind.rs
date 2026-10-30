use serde::{Deserialize, Serialize};

pub(crate) const CLAUDE_QUESTION_TOOL: &str = "AskUserQuestion";
pub(crate) const CLAUDE_PLAN_TOOL: &str = "ExitPlanMode";

const CLAUDE_TOOLS: [(&str, ToolKind); 17] = [
    ("Bash", ToolKind::Shell),
    ("BashOutput", ToolKind::Shell),
    ("KillShell", ToolKind::Shell),
    ("Read", ToolKind::Read),
    ("Write", ToolKind::Edit),
    ("Edit", ToolKind::Edit),
    ("MultiEdit", ToolKind::Edit),
    ("NotebookEdit", ToolKind::Edit),
    ("Grep", ToolKind::Search),
    ("Glob", ToolKind::Search),
    ("WebSearch", ToolKind::Search),
    ("WebFetch", ToolKind::Fetch),
    ("Task", ToolKind::Subagent),
    ("Agent", ToolKind::Subagent),
    ("TodoWrite", ToolKind::Checklist),
    (CLAUDE_QUESTION_TOOL, ToolKind::Question),
    (CLAUDE_PLAN_TOOL, ToolKind::Plan),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    Shell,
    Read,
    Edit,
    Search,
    Fetch,
    Subagent,
    Checklist,
    Question,
    Plan,
    Other,
}

impl ToolKind {
    #[must_use]
    pub fn of_claude_code(name: &str) -> Self {
        CLAUDE_TOOLS
            .iter()
            .find(|(tool, _)| *tool == name)
            .map_or(Self::Other, |(_, kind)| *kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_tools_are_told_apart_by_what_they_do() {
        assert_eq!(ToolKind::of_claude_code("Bash"), ToolKind::Shell);
        assert_eq!(ToolKind::of_claude_code("Write"), ToolKind::Edit);
        assert_eq!(ToolKind::of_claude_code("Glob"), ToolKind::Search);
        assert_eq!(ToolKind::of_claude_code("Task"), ToolKind::Subagent);
        assert_eq!(ToolKind::of_claude_code("TodoWrite"), ToolKind::Checklist);
        assert_eq!(
            ToolKind::of_claude_code("AskUserQuestion"),
            ToolKind::Question
        );
        assert_eq!(
            ToolKind::of_claude_code("mcp__linear__create"),
            ToolKind::Other
        );
    }
}
