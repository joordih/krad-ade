use krad_domain::{Checklist, ToolCall, ToolKind, ToolUseId};
use std::borrow::Cow;

use super::agent::{self, Task, AGENT_LABEL, AGENT_TOOL};
use super::checklist::{self, CHECKLIST_TOOL};
use super::plan::{self, PLAN_LABEL, PLAN_TOOL};
use super::questions::{self, QUESTIONS_LABEL, QUESTIONS_TOOL};
use super::tools::{Action, Workspace};
use super::wire::Call;
use crate::checklist::TODO_TOOL;
use crate::control::Question;

pub(crate) enum EngineCall {
    Questions(Result<Vec<Question>, String>),
    Checklist(Result<Checklist, String>),
    Agent(Result<Task, String>),
    Plan(Result<String, String>),
    Tool(Action),
    Unparsed { name: String, error: String },
}

fn summary_of<T>(parsed: &Result<T, String>, summary: impl FnOnce(&T) -> String) -> String {
    parsed.as_ref().map_or_else(|_| String::new(), summary)
}

impl EngineCall {
    pub(crate) fn parse(call: &Call) -> Self {
        let arguments = &call.arguments;
        match call.name.as_str() {
            QUESTIONS_TOOL => Self::Questions(questions::parse(arguments)),
            CHECKLIST_TOOL => Self::Checklist(checklist::parse(arguments)),
            AGENT_TOOL => Self::Agent(agent::parse(arguments)),
            PLAN_TOOL => Self::Plan(plan::parse(arguments)),
            name => Action::parse(name, arguments).map_or_else(
                |error| Self::Unparsed {
                    name: name.to_owned(),
                    error,
                },
                Self::Tool,
            ),
        }
    }

    pub(crate) fn row(&self, id: &str, workspace: &Workspace) -> ToolCall {
        let (name, kind, summary) = match self {
            Self::Questions(asked) => (
                Cow::Borrowed(QUESTIONS_LABEL),
                ToolKind::Question,
                summary_of(asked, |asked| questions::summary(asked)),
            ),
            Self::Checklist(list) => (
                Cow::Borrowed(TODO_TOOL),
                ToolKind::Checklist,
                summary_of(list, checklist::summary),
            ),
            Self::Agent(task) => (
                Cow::Borrowed(AGENT_LABEL),
                ToolKind::Subagent,
                summary_of(task, |task| task.description.clone()),
            ),
            Self::Plan(proposed) => (
                Cow::Borrowed(PLAN_LABEL),
                ToolKind::Plan,
                summary_of(proposed, |proposed| plan::summary(proposed)),
            ),
            Self::Tool(action) => (action.label(), action.kind(), action.summary(workspace)),
            Self::Unparsed { name, .. } => {
                (Cow::Owned(name.clone()), ToolKind::Other, String::new())
            }
        };
        ToolCall {
            id: Some(ToolUseId::from(id)),
            name: name.into_owned(),
            kind,
            summary,
            edit: None,
        }
    }
}
