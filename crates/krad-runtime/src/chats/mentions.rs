use krad_domain::Host;
use krad_host::HostLink;
use krad_protocol::{Answer, ErrorCode, EventKind, FoundMentions, RequestId};
use krad_repo::{index_files, FileIndex, RepoError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::jobs::{JobDone, Jobs};

const FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct IndexKey {
    host: Host,
    root: PathBuf,
}

#[derive(Debug)]
struct Indexed {
    index: FileIndex,
    built: Instant,
    stale: bool,
}

impl Indexed {
    fn is_fresh(&self, now: Instant) -> bool {
        !self.stale && now.saturating_duration_since(self.built) < FRESH_FOR
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MentionQuery {
    pub(crate) request: RequestId,
    pub(crate) query: String,
    pub(crate) limit: usize,
}

impl MentionQuery {
    fn answer(&self, index: &FileIndex) -> EventKind {
        let entries = index
            .search(&self.query, self.limit)
            .into_iter()
            .cloned()
            .collect();
        self.answered(Answer::Mentions(FoundMentions { entries }))
    }

    fn answered(&self, answer: Answer) -> EventKind {
        EventKind::Answered {
            request: self.request,
            answer: Box::new(answer),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MentionIndexes {
    built: HashMap<IndexKey, Indexed>,
    waiting: HashMap<IndexKey, Vec<MentionQuery>>,
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
        query: MentionQuery,
        (jobs, outbox): (&Jobs, &mut Vec<EventKind>),
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
            return outbox.push(query.answer(&fresh.index));
        }
        let waiters = self.waiting.entry(key.clone()).or_default();
        waiters.push(query);
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
            let failed = FilesIndexed {
                key,
                index: Err(RepoError::Io(error)),
            };
            self.indexed(failed, Instant::now(), outbox);
        }
    }

    pub(crate) fn indexed(
        &mut self,
        done: FilesIndexed,
        now: Instant,
        outbox: &mut Vec<EventKind>,
    ) {
        let FilesIndexed { key, index } = done;
        let waiters = self.waiting.remove(&key).unwrap_or_default();
        match index {
            Ok(index) => {
                outbox.extend(waiters.iter().map(|query| query.answer(&index)));
                let indexed = Indexed {
                    index,
                    built: now,
                    stale: false,
                };
                self.built.insert(key, indexed);
            }
            Err(error) => {
                let message = error.to_string();
                outbox.extend(waiters.iter().map(|query| {
                    query.answered(Answer::Failed {
                        code: ErrorCode::Unavailable,
                        message: message.clone(),
                    })
                }));
            }
        }
    }

    pub(crate) fn forget(&mut self) {
        for indexed in self.built.values_mut() {
            indexed.stale = true;
        }
    }

    pub(crate) fn drop_roots(&mut self, host: &Host, roots: &[PathBuf]) {
        self.built
            .retain(|key, _| key.host != *host || !roots.contains(&key.root));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> IndexKey {
        IndexKey {
            host: Host::Local,
            root: PathBuf::from("/work"),
        }
    }

    fn query(request: u64) -> MentionQuery {
        MentionQuery {
            request: RequestId(request),
            query: "main".to_owned(),
            limit: 4,
        }
    }

    #[test]
    fn an_index_answers_everyone_waiting_and_stays_fresh_until_it_ages_or_is_forgotten() {
        let mut indexes = MentionIndexes::default();
        indexes.waiting.insert(key(), vec![query(1), query(2)]);
        let now = Instant::now();
        let files = FileIndex::new([PathBuf::from("src/main.rs")]);
        let mut outbox = Vec::new();
        indexes.indexed(
            FilesIndexed {
                key: key(),
                index: Ok(files.clone()),
            },
            now,
            &mut outbox,
        );
        let answered: Vec<RequestId> = outbox
            .iter()
            .filter_map(|event| match event {
                EventKind::Answered { request, answer } => {
                    matches!(**answer, Answer::Mentions(ref found) if found.entries.len() == 1)
                        .then_some(*request)
                }
                _ => None,
            })
            .collect();
        assert_eq!(answered, [RequestId(1), RequestId(2)]);
        let kept = &indexes.built[&key()];
        assert!(kept.is_fresh(now));
        assert!(!kept.is_fresh(now + FRESH_FOR));
        indexes.forget();
        assert!(!indexes.built[&key()].is_fresh(now));
        indexes.drop_roots(&Host::Local, &[PathBuf::from("/work")]);
        assert!(indexes.built.is_empty());
    }
}
