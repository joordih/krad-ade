mod graph_cell;
mod rows;
mod view;

use gpui_kit::{AppContext as _, Context};
use krad_domain::review::CommitSha;
use krad_domain::{commit_graph, GraphRow, HistoryCommit, Host, SidePanelSection};
use krad_repo::{commit_files, commit_log, CommitFile};
use std::collections::HashMap;
use std::path::PathBuf;

use super::super::super::EnvironmentShell;

const HISTORY_LIMIT: usize = 400;
const HEAD_REVISION: &str = "HEAD";

#[derive(Clone, Debug, PartialEq, Eq)]
struct HistoryKey {
    host: Host,
    repo: PathBuf,
    head: Option<String>,
    upstream: Option<String>,
}

struct ReadHistory {
    commits: Vec<HistoryCommit>,
    graph: Vec<GraphRow>,
    lanes: usize,
}

#[derive(Default)]
enum HistoryLoad {
    #[default]
    Idle,
    Reading,
    Ready(ReadHistory),
    Failed(String),
}

enum FilesLoad {
    Reading,
    Ready(Vec<CommitFile>),
    Failed(String),
}

#[derive(Default)]
pub(in crate::environment_shell) struct CommitHistory {
    read_for: Option<HistoryKey>,
    load: HistoryLoad,
    expanded: HashMap<CommitSha, FilesLoad>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryRow {
    Commit(usize),
    File { commit: usize, file: usize },
    Note(usize),
}

fn history_rows(
    commits: &[HistoryCommit],
    expanded: &HashMap<CommitSha, FilesLoad>,
) -> Vec<HistoryRow> {
    let mut rows = Vec::with_capacity(commits.len());
    for (index, commit) in commits.iter().enumerate() {
        rows.push(HistoryRow::Commit(index));
        match expanded.get(&commit.sha) {
            None => {}
            Some(FilesLoad::Ready(files)) if !files.is_empty() => {
                rows.extend((0..files.len()).map(|file| HistoryRow::File {
                    commit: index,
                    file,
                }));
            }
            Some(_) => rows.push(HistoryRow::Note(index)),
        }
    }
    rows
}

impl EnvironmentShell {
    fn history_key(&self) -> Option<HistoryKey> {
        let git = self.group_git()?;
        Some(HistoryKey {
            host: self.envs.get(&self.active)?.data.host.clone(),
            repo: git.repo_root.clone(),
            head: git.status.head.clone(),
            upstream: git.status.upstream.clone(),
        })
    }

    fn history_shown(&self) -> bool {
        self.side_layout().is_some_and(|layout| {
            layout.history_open && layout.section == SidePanelSection::Changes
        })
    }

    pub(in crate::environment_shell) fn sync_history(&mut self, cx: &mut Context<Self>) {
        if !self.history_shown() {
            return;
        }
        let key = self.history_key();
        if key == self.history.read_for {
            return;
        }
        let same_repo = key
            .as_ref()
            .zip(self.history.read_for.as_ref())
            .is_some_and(|(key, read)| key.host == read.host && key.repo == read.repo);
        if !same_repo {
            self.history.expanded.clear();
        }
        self.history.read_for.clone_from(&key);
        let Some(key) = key else {
            self.history.load = HistoryLoad::Idle;
            return;
        };
        if key.head.is_none() {
            self.history.load = HistoryLoad::Ready(ReadHistory {
                commits: Vec::new(),
                graph: Vec::new(),
                lanes: 0,
            });
            return;
        }
        self.history.load = HistoryLoad::Reading;
        let link = self.active_link();
        let reading = cx.background_spawn(async move {
            let mut revisions = vec![HEAD_REVISION];
            revisions.extend(key.upstream.as_deref());
            commit_log(&link, &key.repo, &revisions, HISTORY_LIMIT).map(|commits| {
                let graph = commit_graph(&commits);
                let lanes = graph.iter().map(|row| row.width).max().unwrap_or_default();
                (
                    key,
                    ReadHistory {
                        commits,
                        graph,
                        lanes,
                    },
                )
            })
        });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, cx| {
                match read {
                    Ok((key, read)) if shell.history.read_for.as_ref() == Some(&key) => {
                        shell.history.load = HistoryLoad::Ready(read);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::debug!(error = %error, "the commit history could not be read");
                        shell.history.load = HistoryLoad::Failed(error.to_string());
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn toggle_commit(&mut self, sha: &CommitSha, cx: &mut Context<Self>) {
        cx.notify();
        if self.history.expanded.remove(sha).is_some() {
            return;
        }
        let Some(repo) = self.history.read_for.as_ref().map(|key| key.repo.clone()) else {
            return;
        };
        self.history
            .expanded
            .insert(sha.clone(), FilesLoad::Reading);
        let link = self.active_link();
        let asked = sha.clone();
        let reading = cx.background_spawn(async move { commit_files(&link, &repo, &asked) });
        let sha = sha.clone();
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            this.update(cx, |shell, cx| {
                if let Some(files) = shell.history.expanded.get_mut(&sha) {
                    *files = match read {
                        Ok(files) => FilesLoad::Ready(files),
                        Err(error) => FilesLoad::Failed(error.to_string()),
                    };
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn history_repo(&self) -> Option<&PathBuf> {
        self.history.read_for.as_ref().map(|key| &key.repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::{ChangeKind, FileChange, LineCount};
    use std::time::SystemTime;

    fn commit(sha: &str) -> HistoryCommit {
        HistoryCommit {
            sha: CommitSha(sha.to_owned()),
            parents: Vec::new(),
            author: String::new(),
            time: SystemTime::UNIX_EPOCH,
            labels: Vec::new(),
            subject: String::new(),
        }
    }

    fn file(path: &str) -> CommitFile {
        CommitFile {
            change: FileChange {
                path: PathBuf::from(path),
                kind: ChangeKind::Modified,
                origin: None,
            },
            lines: LineCount::default(),
        }
    }

    #[test]
    fn an_expanded_commit_lists_its_files_or_a_note_under_it() {
        let commits = [commit("a"), commit("b"), commit("c")];
        let expanded = HashMap::from([
            (
                CommitSha("a".to_owned()),
                FilesLoad::Ready(vec![file("x"), file("y")]),
            ),
            (CommitSha("c".to_owned()), FilesLoad::Reading),
        ]);
        assert_eq!(
            history_rows(&commits, &expanded),
            [
                HistoryRow::Commit(0),
                HistoryRow::File { commit: 0, file: 0 },
                HistoryRow::File { commit: 0, file: 1 },
                HistoryRow::Commit(1),
                HistoryRow::Commit(2),
                HistoryRow::Note(2),
            ]
        );
    }
}
