use gpui_kit::{AppContext as _, Context};
use krad_repo::{
    replace_in_file, undo_replace, FileMatches, ProjectMatches, ReplaceUndo, ReplacedFile,
    UndoOutcome,
};
use std::path::Path;

use super::super::super::EnvironmentShell;
use super::SearchResults;

const UNDO_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const REPLACED_TEXT: &str = "Replaced";
const OCCURRENCES_TEXT: &str = "occurrences in";
const ONE_FILE_TEXT: &str = "file";
const FILES_TEXT: &str = "files";
const ONE_MATCH_TEXT: &str = "match";
const MATCHES_TEXT: &str = "matches";
const CHANGED_LINES_TEXT: &str = "lines changed since the search were left as they are";
const FAILED_TEXT: &str = "Could not replace in";
const NO_UNDO_TEXT: &str = "This replacement is too large to undo; review it with git";
const TRUNCATED_TEXT: &str = "The search stopped at its result limit, so Replace all would miss \
     some files; narrow it with the files to include or exclude first";
const UNDONE_TEXT: &str = "Restored";
const UNDO_CHANGED_TEXT: &str = "edited since the replacement were left as they are";
const UNDO_FAILED_TEXT: &str = "Could not restore";

const fn files_word(count: usize) -> &'static str {
    if count == 1 {
        ONE_FILE_TEXT
    } else {
        FILES_TEXT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReplacePlan {
    files: usize,
    matches: usize,
}

impl ReplacePlan {
    pub(super) fn question(self) -> String {
        let matches = if self.matches == 1 {
            ONE_MATCH_TEXT
        } else {
            MATCHES_TEXT
        };
        format!(
            "Replace {} {matches} in {} {}?",
            self.matches,
            self.files,
            files_word(self.files)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplaceRefusal {
    Truncated,
    NothingFound,
}

fn plan_replace_all(matches: &ProjectMatches) -> Result<ReplacePlan, ReplaceRefusal> {
    if matches.truncated {
        return Err(ReplaceRefusal::Truncated);
    }
    let plan = ReplacePlan {
        files: matches.files.len(),
        matches: matches.match_count(),
    };
    if plan.matches == 0 {
        return Err(ReplaceRefusal::NothingFound);
    }
    Ok(plan)
}

fn bounded_undo(undos: Vec<ReplaceUndo>) -> Option<Vec<ReplaceUndo>> {
    let size: usize = undos.iter().map(ReplaceUndo::size).sum();
    (size <= UNDO_LIMIT_BYTES && !undos.is_empty()).then_some(undos)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ReplaceReport {
    replaced: usize,
    files: usize,
    changed_lines: usize,
    failures: Vec<String>,
    undos: Vec<ReplaceUndo>,
}

impl ReplaceReport {
    fn add(&mut self, file: ReplacedFile) {
        let outcome = file.outcome;
        self.replaced += outcome.replaced;
        self.changed_lines += outcome.changed_lines;
        if outcome.replaced > 0 {
            self.files += 1;
        }
        self.undos.extend(file.undo);
    }

    fn messages(&self, undo_kept: bool) -> Vec<String> {
        let mut messages = vec![format!(
            "{REPLACED_TEXT} {} {OCCURRENCES_TEXT} {} {}",
            self.replaced,
            self.files,
            files_word(self.files)
        )];
        if self.changed_lines > 0 {
            messages.push(format!("{} {CHANGED_LINES_TEXT}", self.changed_lines));
        }
        messages.extend(
            self.failures
                .iter()
                .map(|failure| format!("{FAILED_TEXT} {failure}")),
        );
        if self.replaced > 0 && !undo_kept {
            messages.push(NO_UNDO_TEXT.to_owned());
        }
        messages
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct UndoReport {
    restored: usize,
    changed_since: usize,
    failures: Vec<String>,
}

impl UndoReport {
    fn messages(&self) -> Vec<String> {
        let mut messages = vec![format!(
            "{UNDONE_TEXT} {} {}",
            self.restored,
            files_word(self.restored)
        )];
        if self.changed_since > 0 {
            messages.push(format!(
                "{} {} {UNDO_CHANGED_TEXT}",
                self.changed_since,
                files_word(self.changed_since)
            ));
        }
        messages.extend(
            self.failures
                .iter()
                .map(|failure| format!("{UNDO_FAILED_TEXT} {failure}")),
        );
        messages
    }
}

impl EnvironmentShell {
    pub(super) fn replace_in_found_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        let SearchResults::Found { matches, .. } = &self.side_search.results else {
            return;
        };
        let files: Vec<FileMatches> = matches
            .files
            .iter()
            .filter(|file| file.path == path)
            .cloned()
            .collect();
        self.replace_found(files, cx);
    }

    pub(super) fn ask_replace_in_all_files(&mut self, cx: &mut Context<Self>) {
        let SearchResults::Found { matches, .. } = &self.side_search.results else {
            return;
        };
        match plan_replace_all(matches) {
            Ok(plan) => self.side_search.confirm_replace = Some(plan),
            Err(ReplaceRefusal::Truncated) => self.push_toast(TRUNCATED_TEXT.to_owned()),
            Err(ReplaceRefusal::NothingFound) => {}
        }
        cx.notify();
    }

    pub(super) fn cancel_replace_in_all_files(&mut self, cx: &mut Context<Self>) {
        self.side_search.confirm_replace = None;
        cx.notify();
    }

    pub(super) fn replace_in_all_files(&mut self, cx: &mut Context<Self>) {
        let Some(confirmed) = self.side_search.confirm_replace.take() else {
            return;
        };
        let SearchResults::Found { matches, .. } = &self.side_search.results else {
            return;
        };
        if plan_replace_all(matches) != Ok(confirmed) {
            cx.notify();
            return;
        }
        let files = matches.files.clone();
        self.replace_found(files, cx);
    }

    fn replace_found(&mut self, files: Vec<FileMatches>, cx: &mut Context<Self>) {
        if files.is_empty() || self.side_search.replacing {
            return;
        }
        self.side_search.replacing = true;
        self.side_search.last_replace = None;
        let replacement = self.side_search.replacement.clone();
        let link = self.active_link();
        cx.spawn(async move |this, cx| {
            let report = cx
                .background_spawn(async move {
                    let mut report = ReplaceReport::default();
                    for file in &files {
                        match replace_in_file(&link, file, &replacement) {
                            Ok(replaced) => report.add(replaced),
                            Err(error) => report
                                .failures
                                .push(format!("{}: {error}", file.path.display())),
                        }
                    }
                    report
                })
                .await;
            this.update(cx, |shell, cx| {
                shell.side_search.replacing = false;
                let undo = bounded_undo(report.undos.clone());
                for message in report.messages(undo.is_some()) {
                    shell.push_toast(message);
                }
                shell.side_search.last_replace = undo;
                shell.rerun_side_search(cx);
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn undo_last_replace(&mut self, cx: &mut Context<Self>) {
        if self.side_search.replacing {
            return;
        }
        let Some(undos) = self.side_search.last_replace.take() else {
            return;
        };
        self.side_search.replacing = true;
        let link = self.active_link();
        cx.spawn(async move |this, cx| {
            let report = cx
                .background_spawn(async move {
                    let mut report = UndoReport::default();
                    for undo in &undos {
                        match undo_replace(&link, undo) {
                            Ok(UndoOutcome::Restored) => report.restored += 1,
                            Ok(UndoOutcome::ChangedSince) => report.changed_since += 1,
                            Err(error) => report
                                .failures
                                .push(format!("{}: {error}", undo.path.display())),
                        }
                    }
                    report
                })
                .await;
            this.update(cx, |shell, cx| {
                shell.side_search.replacing = false;
                for message in report.messages() {
                    shell.push_toast(message);
                }
                shell.rerun_side_search(cx);
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::{FoundLine, Replacement};
    use std::path::PathBuf;

    fn file(path: &str, matches: usize) -> FileMatches {
        FileMatches {
            path: PathBuf::from(path),
            lines: vec![FoundLine {
                line: 1,
                text: "foo foo foo".to_owned(),
                matches: (0..matches).map(|index| index * 4..index * 4 + 3).collect(),
            }],
        }
    }

    #[test]
    fn replace_all_asks_with_the_file_and_match_counts() {
        let matches = ProjectMatches {
            files: vec![file("a.rs", 2), file("b.rs", 1)],
            truncated: false,
        };
        let plan = plan_replace_all(&matches);
        assert_eq!(
            plan,
            Ok(ReplacePlan {
                files: 2,
                matches: 3
            })
        );
        assert_eq!(
            plan.map(ReplacePlan::question),
            Ok("Replace 3 matches in 2 files?".to_owned())
        );
    }

    #[test]
    fn replace_all_refuses_a_truncated_search() {
        let matches = ProjectMatches {
            files: vec![file("a.rs", 2)],
            truncated: true,
        };
        assert_eq!(plan_replace_all(&matches), Err(ReplaceRefusal::Truncated));
        assert_eq!(
            plan_replace_all(&ProjectMatches::default()),
            Err(ReplaceRefusal::NothingFound)
        );
    }

    #[test]
    fn the_report_counts_files_with_replacements_and_lists_problems() {
        let mut report = ReplaceReport::default();
        report.add(ReplacedFile {
            outcome: Replacement {
                replaced: 3,
                changed_lines: 0,
            },
            undo: None,
        });
        report.add(ReplacedFile {
            outcome: Replacement {
                replaced: 0,
                changed_lines: 2,
            },
            undo: None,
        });
        report.failures.push("a.rs: denied".to_owned());
        assert_eq!(
            report.messages(true),
            [
                "Replaced 3 occurrences in 1 file",
                "2 lines changed since the search were left as they are",
                "Could not replace in a.rs: denied",
            ]
        );
        assert_eq!(
            report.messages(false).last().map(String::as_str),
            Some(NO_UNDO_TEXT)
        );
    }

    #[test]
    fn nothing_replaced_keeps_no_undo() {
        assert_eq!(bounded_undo(Vec::new()), None);
    }

    #[test]
    fn the_undo_report_counts_restored_and_edited_files() {
        let report = UndoReport {
            restored: 2,
            changed_since: 1,
            failures: Vec::new(),
        };
        assert_eq!(
            report.messages(),
            [
                "Restored 2 files",
                "1 file edited since the replacement were left as they are",
            ]
        );
    }
}
