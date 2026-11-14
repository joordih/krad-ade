use gpui_kit::Context;
use krad_client::ClientError;
use krad_domain::review::CommitSha;
use krad_domain::{
    ChangeKind, EnvironmentName, FileChange, FileOpening, FileView, NodeEdit, TerminalId,
};
use krad_protocol::{Answer, DiffOf, RepoAsk};
use std::path::{Path, PathBuf};

use super::super::review_view::numbered_lines;
use super::super::session_changes::LoadGeneration;
use super::super::EnvironmentShell;
use super::revert::block_starts;
use super::{DiffColors, DiffLoad, FileDiff, ReadDiff};

enum DiffSource {
    Working(Option<(PathBuf, FileChange)>),
    Commit {
        repo: PathBuf,
        sha: CommitSha,
        change: FileChange,
    },
}

impl DiffSource {
    fn ask(self) -> Option<RepoAsk> {
        let (repo, of) = match self {
            Self::Working(None) => return None,
            Self::Working(Some((repo, change))) => (repo, DiffOf::Working { change }),
            Self::Commit { repo, sha, change } => (repo, DiffOf::Commit { sha, change }),
        };
        Some(RepoAsk::Diff { repo, of })
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_commit_file(
        &mut self,
        path: &Path,
        sha: CommitSha,
        opening: FileOpening,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(group) = self.envs.get(&env).map(|live| live.selected_group) else {
            return;
        };
        let opened = NodeEdit::OpenFile {
            group,
            path: path.to_path_buf(),
            view: FileView::Commit(sha),
            opening,
        };
        let Some(id) = self.edit_environment(&env, opened, cx).opened() else {
            return;
        };
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        live.focused = Some(id);
        cx.notify();
        self.save_view();
    }

    pub(in crate::environment_shell) fn sync_changes_tabs(&mut self, cx: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let open: Vec<TerminalId> = live
            .data
            .nodes
            .iter()
            .filter(|node| node.file.is_some())
            .map(|node| node.id)
            .collect();
        live.diffs.retain(|id, _| open.contains(id));
        let Some((id, path, view)) = live.shown_diff() else {
            return;
        };
        let diff = live
            .diffs
            .entry(id)
            .or_insert_with(|| FileDiff::new(path.clone(), view.clone()));
        if diff.path != path || diff.view != view {
            let generation = diff.generation;
            *diff = FileDiff::new(path.clone(), view.clone());
            diff.generation = generation;
        }
        match diff.load {
            DiffLoad::Pending => {}
            DiffLoad::Ready(_) => return self.paint_diff(&env, id, cx),
            DiffLoad::Reading | DiffLoad::Failed(_) => return,
        }
        diff.load = DiffLoad::Reading;
        let generation = diff.generation.advance();
        let source = match view {
            FileView::Commit(sha) => {
                let Some(repo) = self.repo_holding(&path) else {
                    return;
                };
                let Ok(relative) = path.strip_prefix(&repo).map(Path::to_path_buf) else {
                    return;
                };
                DiffSource::Commit {
                    repo,
                    sha,
                    change: FileChange {
                        path: relative,
                        kind: ChangeKind::Modified,
                        origin: None,
                    },
                }
            }
            FileView::Changes | FileView::Edit => DiffSource::Working(self.change_at(&path)),
        };
        self.read_diff(env, id, generation, source, cx);
    }

    pub(in crate::environment_shell) fn forget_diffs(&mut self) {
        for live in self.envs.values_mut() {
            for diff in live.diffs.values_mut() {
                if diff.view == FileView::Changes {
                    diff.load = DiffLoad::Pending;
                }
            }
        }
    }

    fn repo_holding(&self, path: &Path) -> Option<PathBuf> {
        let host = &self.envs.get(&self.active)?.data.host;
        self.runtime
            .git
            .on(host)
            .filter(|git| path.starts_with(&git.repo_root))
            .map(|git| git.repo_root.clone())
            .max_by_key(|root| root.components().count())
    }

    fn change_at(&self, path: &Path) -> Option<(PathBuf, FileChange)> {
        let host = &self.envs.get(&self.active)?.data.host;
        self.runtime.git.on(host).find_map(|git| {
            git.status
                .changes
                .iter()
                .find(|change| git.repo_root.join(&change.path) == path)
                .map(|change| (git.repo_root.clone(), change.clone()))
        })
    }

    fn read_diff(
        &self,
        env: EnvironmentName,
        id: TerminalId,
        generation: LoadGeneration,
        source: DiffSource,
        cx: &mut Context<Self>,
    ) {
        let asked = source.ask().map(|ask| {
            self.ask_repo(
                &env,
                ask,
                |answer| match answer {
                    Answer::Diff { text } => Some(text),
                    _ => None,
                },
                cx,
            )
        });
        let executor = cx.background_executor().clone();
        let reading = async move {
            let Some(asked) = asked else {
                return Ok(Vec::new());
            };
            let text = asked.await?;
            Ok::<_, ClientError>(executor.spawn(async move { numbered_lines(&text) }).await)
        };
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, cx| {
                if let Some(diff) = shell
                    .envs
                    .get_mut(&env)
                    .and_then(|live| live.diffs.get_mut(&id))
                    .filter(|diff| diff.generation.admits(generation))
                {
                    let revertible = diff.view == FileView::Changes;
                    diff.load = match read {
                        Ok(lines) => DiffLoad::Ready(ReadDiff {
                            blocks: if revertible {
                                block_starts(&lines)
                            } else {
                                Vec::new()
                            },
                            lines,
                            rows: Vec::new(),
                            widest: 0,
                            colors: DiffColors::Unpainted,
                        }),
                        Err(error) => DiffLoad::Failed(error.to_string()),
                    };
                    diff.refold();
                    diff.relocate_draft();
                }
                shell.refresh_diff_comments(&env, id, cx);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}
