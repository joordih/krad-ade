use krad_domain::{Checklist, ChecklistItem, ChecklistStatus};
use serde::Deserialize;
use serde_json::Value;

pub(crate) const TODO_TOOL: &str = "TodoWrite";

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TodoStatus {
    #[serde(alias = "inProgress")]
    InProgress,
    Completed,
    Cancelled,
    #[serde(other)]
    Pending,
}

impl From<TodoStatus> for ChecklistStatus {
    fn from(status: TodoStatus) -> Self {
        match status {
            TodoStatus::Pending => Self::Pending,
            TodoStatus::InProgress => Self::InProgress,
            TodoStatus::Completed | TodoStatus::Cancelled => Self::Done,
        }
    }
}

#[derive(Deserialize)]
struct Todo {
    content: String,
    status: TodoStatus,
}

#[derive(Deserialize)]
struct Todos {
    todos: Vec<Todo>,
}

pub(crate) fn todos(input: &Value) -> Option<Checklist> {
    let Todos { todos } = Todos::deserialize(input).ok()?;
    Some(
        todos
            .into_iter()
            .map(|todo| ChecklistItem {
                text: todo.content,
                status: todo.status.into(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn statuses(list: &Checklist) -> Vec<(&str, ChecklistStatus)> {
        list.items
            .iter()
            .map(|item| (item.text.as_str(), item.status))
            .collect()
    }

    #[test]
    fn claude_code_todos_become_the_checklist() {
        let list = todos(&json!({"todos": [
            {"content": "Read the parser", "status": "completed", "activeForm": "Reading the parser"},
            {"content": "Fix the bug", "status": "in_progress", "activeForm": "Fixing the bug"},
            {"content": "Run the tests", "status": "pending", "activeForm": "Running the tests"}
        ]}))
        .unwrap_or_default();
        assert_eq!(
            statuses(&list),
            [
                ("Read the parser", ChecklistStatus::Done),
                ("Fix the bug", ChecklistStatus::InProgress),
                ("Run the tests", ChecklistStatus::Pending),
            ]
        );
    }

    #[test]
    fn opencode_todos_with_priorities_and_cancelled_items_become_the_checklist() {
        let list = todos(&json!({"todos": [
            {"content": "Map the UI", "status": "cancelled", "priority": "low", "id": "1"},
            {"content": "Draw it", "status": "in_progress", "priority": "high", "id": "2"},
            {"content": "Ship it", "status": "someday", "priority": "medium", "id": "3"}
        ]}))
        .unwrap_or_default();
        assert_eq!(
            statuses(&list),
            [
                ("Map the UI", ChecklistStatus::Done),
                ("Draw it", ChecklistStatus::InProgress),
                ("Ship it", ChecklistStatus::Pending),
            ]
        );
        assert_eq!(todos(&json!({"plan": []})), None);
    }
}
