use gpui_kit::Context;
use krad_domain::{EntryKind, EnvironmentName, IndexEntry, ToastKind};
use krad_protocol::{Answer, ChatCommand, Command};
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum FilesFor {
    Suggestions,
    Palette,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct FilesQuery {
    environment: EnvironmentName,
    root: PathBuf,
    query: String,
    limit: usize,
}

pub(in crate::environment_shell) struct FoundFiles {
    asked: FilesQuery,
    entries: Vec<IndexEntry>,
    stale: bool,
}

impl FoundFiles {
    fn answers(&self, query: &FilesQuery) -> bool {
        !self.stale && self.asked == *query
    }
}

#[derive(Default)]
pub(in crate::environment_shell) struct FileSearches {
    suggestions: Option<FoundFiles>,
    asked: [Option<FilesQuery>; 2],
}

impl FileSearches {
    const fn slot(wanted: FilesFor) -> usize {
        match wanted {
            FilesFor::Suggestions => 0,
            FilesFor::Palette => 1,
        }
    }

    fn is_latest(&self, wanted: FilesFor, query: &FilesQuery) -> bool {
        self.asked[Self::slot(wanted)].as_ref() == Some(query)
    }
}

fn files_under(root: &Path, entries: Vec<IndexEntry>) -> Vec<PathBuf> {
    entries
        .into_iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| root.join(entry.path))
        .collect()
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn search_files(
        &mut self,
        (root, query, limit): (PathBuf, &str, usize),
        wanted: FilesFor,
        cx: &mut Context<Self>,
    ) {
        let asked = FilesQuery {
            environment: self.active.clone(),
            root,
            query: query.to_owned(),
            limit,
        };
        let searches = &self.chat.files;
        let known = searches.is_latest(wanted, &asked)
            || (wanted == FilesFor::Suggestions
                && searches
                    .suggestions
                    .as_ref()
                    .is_some_and(|found| found.answers(&asked)));
        if known || !self.host_ready(&self.active, cx) {
            return;
        }
        self.chat.files.asked[FileSearches::slot(wanted)] = Some(asked.clone());
        let command = Command::Chat(ChatCommand::Mentions {
            environment: asked.environment.clone(),
            root: asked.root.clone(),
            query: asked.query.clone(),
            limit,
        });
        let answer = self.runtime.ask(command, cx);
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            this.update(cx, |shell, cx| shell.files_found(asked, wanted, answer, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn files_found(
        &mut self,
        asked: FilesQuery,
        wanted: FilesFor,
        answer: Result<Answer, krad_client::ClientError>,
        cx: &mut Context<Self>,
    ) {
        if !self.chat.files.is_latest(wanted, &asked) {
            return;
        }
        self.chat.files.asked[FileSearches::slot(wanted)] = None;
        let entries = match answer {
            Ok(Answer::Mentions(found)) => found.entries,
            Ok(_) => return,
            Err(failed) => return self.toast(ToastKind::Error, &failed.to_string(), cx),
        };
        match wanted {
            FilesFor::Suggestions => {
                self.chat.files.suggestions = Some(FoundFiles {
                    asked,
                    entries,
                    stale: false,
                });
            }
            FilesFor::Palette => self.palette_files = files_under(&asked.root, entries),
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn forget_file_index(&mut self) {
        if let Some(found) = self.chat.files.suggestions.as_mut() {
            found.stale = true;
        }
        self.runtime
            .client
            .post(Command::Chat(ChatCommand::ForgetMentions));
    }

    pub(in crate::environment_shell) fn suggested_entries(
        &self,
        root: &Path,
    ) -> Option<&[IndexEntry]> {
        let found = self.chat.files.suggestions.as_ref()?;
        let place = &found.asked;
        (place.root == root && place.environment == self.active).then_some(found.entries.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(root: &str, query: &str) -> FilesQuery {
        FilesQuery {
            environment: EnvironmentName::new("main".to_owned()),
            root: PathBuf::from(root),
            query: query.to_owned(),
            limit: 8,
        }
    }

    #[test]
    fn found_files_answer_only_their_own_query_until_git_changes() {
        let mut found = FoundFiles {
            asked: query("/work", "main"),
            entries: Vec::new(),
            stale: false,
        };
        assert!(found.answers(&query("/work", "main")));
        assert!(!found.answers(&query("/work", "mai")));
        assert!(!found.answers(&query("/elsewhere", "main")));
        found.stale = true;
        assert!(!found.answers(&query("/work", "main")));
    }

    #[test]
    fn only_folder_entries_are_dropped_from_the_palette_s_files() {
        let entries = vec![
            IndexEntry {
                path: PathBuf::from("src"),
                kind: EntryKind::Directory,
            },
            IndexEntry {
                path: PathBuf::from("src/main.rs"),
                kind: EntryKind::File,
            },
        ];
        assert_eq!(
            files_under(Path::new("/work"), entries),
            [PathBuf::from("/work/src/main.rs")]
        );
    }
}
