use krad_lsp::{Progress, ProgressState};
use std::collections::BTreeMap;

const PERCENT_SIGN: char = '%';
const ELLIPSIS: char = '…';

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ServerProgress {
    tasks: BTreeMap<String, ProgressTask>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProgressTask {
    title: String,
    percentage: Option<u32>,
}

impl ServerProgress {
    pub(super) fn apply(&mut self, progress: Progress) {
        match progress.state {
            ProgressState::Begin {
                title, percentage, ..
            } => {
                self.tasks
                    .insert(progress.token, ProgressTask { title, percentage });
            }
            ProgressState::Report { percentage, .. } => {
                if let Some(task) = self.tasks.get_mut(&progress.token) {
                    task.percentage = percentage.or(task.percentage);
                }
            }
            ProgressState::End { .. } => {
                self.tasks.remove(&progress.token);
            }
        }
    }

    pub(super) fn label(&self) -> Option<String> {
        let task = self.tasks.values().next()?;
        Some(match task.percentage {
            Some(percentage) => format!("{}{ELLIPSIS} {percentage}{PERCENT_SIGN}", task.title),
            None => format!("{}{ELLIPSIS}", task.title),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(token: &str, state: ProgressState) -> Progress {
        Progress {
            token: token.to_owned(),
            state,
        }
    }

    #[test]
    fn a_task_shows_its_title_and_latest_percentage_until_it_ends() {
        let mut shown = ServerProgress::default();
        assert_eq!(shown.label(), None);
        shown.apply(progress(
            "index",
            ProgressState::Begin {
                title: "Indexing".to_owned(),
                message: None,
                percentage: Some(0),
            },
        ));
        shown.apply(progress(
            "index",
            ProgressState::Report {
                message: None,
                percentage: Some(42),
            },
        ));
        assert_eq!(shown.label().as_deref(), Some("Indexing… 42%"));
        shown.apply(progress("index", ProgressState::End { message: None }));
        assert_eq!(shown.label(), None);
    }

    #[test]
    fn a_report_for_an_unknown_task_is_ignored() {
        let mut shown = ServerProgress::default();
        shown.apply(progress(
            "other",
            ProgressState::Report {
                message: None,
                percentage: Some(10),
            },
        ));
        assert_eq!(shown.label(), None);
    }
}
