use gpui_kit::Context;
use krad_domain::{ChangeKind, EnvironmentName, FileChange, Host, TerminalId};
use krad_protocol::{Answer, DiffOf, RepoAsk};
use std::ops::Range;
use std::path::{Path, PathBuf};

use super::super::language_servers::DiskChange;
use super::super::review_view::{numbered_lines, DiffLine, LineKind};
use super::super::EnvironmentShell;
use super::editor_marks::LineChange;

#[derive(Default)]
struct Block {
    added: Option<Range<usize>>,
    removed: bool,
    at: usize,
}

impl Block {
    fn closed(self, before: Option<usize>) -> Option<LineChange> {
        match (self.added, self.removed) {
            (Some(lines), true) => Some(LineChange {
                lines,
                kind: ChangeKind::Modified,
            }),
            (Some(lines), false) => Some(LineChange {
                lines,
                kind: ChangeKind::Added,
            }),
            (None, true) => {
                let at = before.unwrap_or(self.at);
                Some(LineChange {
                    lines: at..at,
                    kind: ChangeKind::Deleted,
                })
            }
            (None, false) => None,
        }
    }
}

fn line_index(number: Option<u32>) -> Option<usize> {
    usize::try_from(number?).ok()?.checked_sub(1)
}

fn line_changes(diff: &[DiffLine]) -> Vec<LineChange> {
    let mut changes = Vec::new();
    let mut block: Option<Block> = None;
    let mut next = 0;
    for line in diff {
        match line.kind {
            LineKind::Added => {
                let Some(index) = line_index(line.new) else {
                    continue;
                };
                let open = block.get_or_insert_with(|| Block {
                    at: index,
                    ..Block::default()
                });
                open.added = Some(match open.added.take() {
                    Some(lines) => lines.start..index + 1,
                    None => index..index + 1,
                });
                next = index + 1;
            }
            LineKind::Removed => {
                block
                    .get_or_insert_with(|| Block {
                        at: next,
                        ..Block::default()
                    })
                    .removed = true;
            }
            LineKind::Context => {
                let index = line_index(line.new);
                changes.extend(block.take().and_then(|open| open.closed(index)));
                if let Some(index) = index {
                    next = index + 1;
                }
            }
            LineKind::Hunk => changes.extend(block.take().and_then(|open| open.closed(None))),
        }
    }
    changes.extend(block.and_then(|open| open.closed(None)));
    changes
}

impl EnvironmentShell {
    fn change_on(&self, host: &Host, path: &Path) -> Option<(PathBuf, FileChange)> {
        self.runtime.git.on(host).find_map(|git| {
            git.status
                .changes
                .iter()
                .find(|change| git.repo_root.join(&change.path) == path)
                .map(|change| (git.repo_root.clone(), change.clone()))
        })
    }

    pub(super) fn load_change_marks(
        &self,
        env: &EnvironmentName,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.envs.get(env) else {
            return;
        };
        let Some(editor) = live.editors.get(&id).filter(|editor| !editor.dirty) else {
            return;
        };
        let Some(state) = editor.state.clone() else {
            return;
        };
        let marks = editor.marks.clone();
        let Some((repo, change)) = self.change_on(&live.data.host, &editor.path) else {
            marks.show_changes(&[], &state, cx);
            return;
        };
        let ask = RepoAsk::Diff {
            repo,
            of: DiffOf::Working { change },
        };
        let asked = self.ask_repo(
            env,
            ask,
            |answer| match answer {
                Answer::Diff { text } => Some(text),
                _ => None,
            },
            cx,
        );
        let executor = cx.background_executor().clone();
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let text = match asked.await {
                Ok(text) => text,
                Err(error) => {
                    tracing::debug!(%error, "no working diff for the change marks");
                    return;
                }
            };
            let changes = executor
                .spawn(async move { line_changes(&numbered_lines(&text)) })
                .await;
            this.update(cx, |shell, cx| {
                let clean = shell
                    .envs
                    .get(&env)
                    .and_then(|live| live.editors.get(&id))
                    .is_some_and(|editor| !editor.dirty);
                if clean {
                    marks.show_changes(&changes, &state, cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn reload_change_marks(
        &self,
        moved: &[(Host, PathBuf, DiskChange)],
        cx: &mut Context<Self>,
    ) {
        let touched: Vec<(EnvironmentName, TerminalId)> = self
            .envs
            .iter()
            .flat_map(|(name, live)| {
                live.editors
                    .iter()
                    .filter(|(_, editor)| {
                        moved
                            .iter()
                            .any(|(host, path, _)| *host == live.data.host && *path == editor.path)
                    })
                    .map(|(id, _)| (name.clone(), *id))
            })
            .collect();
        for (env, id) in touched {
            self.load_change_marks(&env, id, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "\
@@ -1,4 +1,5 @@
 keep
-old
+new
+extra
 keep
@@ -10,3 +11,2 @@
 keep
-gone
 keep
@@ -20,1 +20,2 @@
 keep
+appended
";

    #[test]
    fn replaced_lines_are_modified_new_lines_added_and_removals_a_boundary() {
        let changes = line_changes(&numbered_lines(DIFF));
        assert_eq!(
            changes,
            [
                LineChange {
                    lines: 1..3,
                    kind: ChangeKind::Modified,
                },
                LineChange {
                    lines: 11..11,
                    kind: ChangeKind::Deleted,
                },
                LineChange {
                    lines: 20..21,
                    kind: ChangeKind::Added,
                },
            ]
        );
    }

    #[test]
    fn removals_at_the_top_of_a_file_mark_its_first_line() {
        let changes = line_changes(&numbered_lines("@@ -1,2 +1,1 @@\n-gone\n keep\n"));
        assert_eq!(
            changes,
            [LineChange {
                lines: 0..0,
                kind: ChangeKind::Deleted,
            }]
        );
    }

    #[test]
    fn a_new_file_is_added_from_its_first_to_its_last_line() {
        let changes = line_changes(&numbered_lines("@@ -0,0 +1,3 @@\n+a\n+b\n+c\n"));
        assert_eq!(
            changes,
            [LineChange {
                lines: 0..3,
                kind: ChangeKind::Added,
            }]
        );
    }
}
