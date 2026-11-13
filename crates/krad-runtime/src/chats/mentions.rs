use futures::channel::oneshot;
use krad_domain::Host;
use krad_host::HostLink;
use krad_repo::{index_files, FileIndex, RepoError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::jobs::{JobDone, Jobs};
use crate::runtime::answer_to;

const FRESH_FOR: Duration = Duration::from_secs(30);

pub(crate) type IndexAnswer = Result<Arc<FileIndex>, Arc<RepoError>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct IndexKey {
    host: Host,
    root: PathBuf,
}

#[derive(Debug)]
struct Indexed {
    index: Arc<FileIndex>,
    built: Instant,
    stale: bool,
}

impl Indexed {
    fn is_fresh(&self, now: Instant) -> bool {
        !self.stale && now.saturating_duration_since(self.built) < FRESH_FOR
    }
}

#[derive(Debug, Default)]
pub(crate) struct MentionIndexes {
    built: HashMap<IndexKey, Indexed>,
    waiting: HashMap<IndexKey, Vec<oneshot::Sender<IndexAnswer>>>,
}

#[derive(Debug)]
pub(crate) struct FilesIndexed {
    key: IndexKey,
    index: Result<FileIndex, RepoError>,
}

impl MentionIndexes {
    pub(crate) fn ask(
        &mut self,
        (link, root): (HostLink, PathBuf),
        reply: oneshot::Sender<IndexAnswer>,
        jobs: &Jobs,
    ) {
        let key = IndexKey {
            host: link.host(),
            root,
        };
        if let Some(fresh) = self
            .built
            .get(&key)
            .filter(|indexed| indexed.is_fresh(Instant::now()))
        {
            return answer_to(reply, Ok(Arc::clone(&fresh.index)));
        }
        let waiters = self.waiting.entry(key.clone()).or_default();
        waiters.push(reply);
        if waiters.len() > 1 {
            return;
        }
        let job_key = key.clone();
        let started = jobs.run(move || {
            let index = index_files(&link, &job_key.root);
            JobDone::FilesIndexed(FilesIndexed {
                key: job_key,
                index,
            })
        });
        if let Err(error) = started {
            self.indexed(
                FilesIndexed {
                    key,
                    index: Err(RepoError::Io(error)),
                },
                Instant::now(),
            );
        }
    }

    pub(crate) fn indexed(&mut self, done: FilesIndexed, now: Instant) {
        let FilesIndexed { key, index } = done;
        let index = index.map(Arc::new).map_err(Arc::new);
        if let Ok(index) = &index {
            self.built.insert(
                key.clone(),
                Indexed {
                    index: Arc::clone(index),
                    built: now,
                    stale: false,
                },
            );
        }
        for reply in self.waiting.remove(&key).unwrap_or_default() {
            answer_to(reply, index.clone());
        }
    }

    pub(crate) fn forget(&mut self) {
        for indexed in self.built.values_mut() {
            indexed.stale = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    fn key() -> IndexKey {
        IndexKey {
            host: Host::Local,
            root: PathBuf::from("/work"),
        }
    }

    #[test]
    fn an_index_answers_everyone_waiting_and_stays_fresh_until_it_ages_or_is_forgotten() {
        let mut indexes = MentionIndexes::default();
        let (first, first_answer) = oneshot::channel();
        let (second, second_answer) = oneshot::channel();
        indexes.waiting.insert(key(), vec![first, second]);
        let now = Instant::now();
        let files = FileIndex::new([PathBuf::from("src/main.rs")]);
        indexes.indexed(
            FilesIndexed {
                key: key(),
                index: Ok(files.clone()),
            },
            now,
        );
        for answer in [first_answer, second_answer] {
            let answered = block_on(answer).ok().and_then(Result::ok);
            assert_eq!(answered.as_deref(), Some(&files));
        }
        let kept = &indexes.built[&key()];
        assert!(kept.is_fresh(now));
        assert!(!kept.is_fresh(now + FRESH_FOR));
        indexes.forget();
        assert!(!indexes.built[&key()].is_fresh(now));
    }
}
