use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecklistStatus {
    Pending,
    InProgress,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub text: String,
    pub status: ChecklistStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Checklist {
    pub items: Vec<ChecklistItem>,
}

impl Checklist {
    #[must_use]
    pub fn done(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ChecklistStatus::Done)
            .count()
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.done() == self.items.len()
    }

    #[must_use]
    pub fn current(&self) -> Option<&ChecklistItem> {
        self.items
            .iter()
            .find(|item| item.status == ChecklistStatus::InProgress)
            .or_else(|| {
                self.items
                    .iter()
                    .find(|item| item.status == ChecklistStatus::Pending)
            })
    }
}

impl FromIterator<ChecklistItem> for Checklist {
    fn from_iter<I: IntoIterator<Item = ChecklistItem>>(items: I) -> Self {
        Self {
            items: items
                .into_iter()
                .filter(|item| !item.text.trim().is_empty())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str, status: ChecklistStatus) -> ChecklistItem {
        ChecklistItem {
            text: text.to_owned(),
            status,
        }
    }

    #[test]
    fn a_checklist_counts_its_done_items_and_names_the_current_one() {
        let list: Checklist = [
            item("Read the parser", ChecklistStatus::Done),
            item("  ", ChecklistStatus::Pending),
            item("Fix the bug", ChecklistStatus::InProgress),
            item("Run the tests", ChecklistStatus::Pending),
        ]
        .into_iter()
        .collect();
        assert_eq!(list.items.len(), 3);
        assert_eq!(list.done(), 1);
        assert!(!list.is_finished());
        assert_eq!(
            list.current().map(|item| item.text.as_str()),
            Some("Fix the bug")
        );
    }

    #[test]
    fn a_checklist_is_stored_as_its_items() {
        let list = Checklist {
            items: vec![item("Ship it", ChecklistStatus::InProgress)],
        };
        let json = serde_json::to_string(&list).unwrap_or_default();
        assert_eq!(json, r#"[{"text":"Ship it","status":"in_progress"}]"#);
        assert_eq!(serde_json::from_str::<Checklist>(&json).ok(), Some(list));
    }
}
