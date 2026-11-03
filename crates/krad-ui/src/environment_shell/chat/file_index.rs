use gpui_kit::{AppContext as _, Context};
use krad_domain::Host;
use krad_repo::{index_files, EntryKind, FileIndex, IndexEntry};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::super::EnvironmentShell;

const FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct IndexKey {
    host: Host,
    root: PathBuf,
}

pub(in crate::environment_shell) struct IndexedFiles {
    key: IndexKey,
    index: FileIndex,
    built: Instant,
    stale: bool,
}

impl IndexedFiles {
    fn is_fresh(&self, key: &IndexKey, now: Instant) -> bool {
        self.key == *key && !self.stale && now.duration_since(self.built) < FRESH_FOR
    }
}

impl EnvironmentShell {
    fn index_key(&self, root: PathBuf) -> Option<IndexKey> {
        let host = self.envs.get(&self.active)?.data.host.clone();
        Some(IndexKey { host, root })
    }

    pub(in crate::environment_shell) fn refresh_file_index(
        &mut self,
        root: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = root.and_then(|root| self.index_key(root)) else {
            return;
        };
        let fresh = self
            .chat
            .files
            .as_ref()
            .is_some_and(|files| files.is_fresh(&key, Instant::now()));
        if fresh || self.chat.indexing.as_ref() == Some(&key) || !self.host_ready(&self.active) {
            return;
        }
        self.chat.indexing = Some(key.clone());
        let link = self.active_link();
        let root = key.root.clone();
        let index = cx.background_spawn(async move { index_files(&link, &root) });
        cx.spawn(async move |this, cx| {
            let index = index.await;
            this.update(cx, |shell, cx| {
                if shell.chat.indexing.as_ref() == Some(&key) {
                    shell.chat.indexing = None;
                }
                match index {
                    Ok(index) => {
                        shell.chat.files = Some(IndexedFiles {
                            key,
                            index,
                            built: Instant::now(),
                            stale: false,
                        });
                        shell.refind_palette_files(cx);
                    }
                    Err(error) => shell.toast_failure(&error),
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn forget_file_index(&mut self) {
        if let Some(files) = self.chat.files.as_mut() {
            files.stale = true;
        }
    }

    pub(in crate::environment_shell) fn indexed_entries(
        &self,
        root: &Path,
        query: &str,
        limit: usize,
    ) -> Option<Vec<&IndexEntry>> {
        let host = &self.envs.get(&self.active)?.data.host;
        let files = self
            .chat
            .files
            .as_ref()
            .filter(|files| files.key.root == root && files.key.host == *host)?;
        Some(files.index.search(query, limit))
    }

    pub(in crate::environment_shell) fn indexed_files(
        &self,
        root: &Path,
        query: &str,
        limit: usize,
    ) -> Vec<PathBuf> {
        self.indexed_entries(root, query, limit)
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| entry.kind == EntryKind::File)
            .map(|entry| root.join(&entry.path))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexed(stale: bool, built: Instant) -> IndexedFiles {
        IndexedFiles {
            key: IndexKey {
                host: Host::Local,
                root: PathBuf::from("/work"),
            },
            index: FileIndex::default(),
            built,
            stale,
        }
    }

    #[test]
    fn an_index_stays_fresh_for_its_root_until_it_ages_or_git_changes() {
        let now = Instant::now();
        let key = IndexKey {
            host: Host::Local,
            root: PathBuf::from("/work"),
        };
        let other = IndexKey {
            host: Host::Local,
            root: PathBuf::from("/elsewhere"),
        };
        assert!(indexed(false, now).is_fresh(&key, now));
        assert!(!indexed(false, now).is_fresh(&other, now));
        assert!(!indexed(true, now).is_fresh(&key, now));
        assert!(!indexed(false, now).is_fresh(&key, now + FRESH_FOR));
    }
}
