mod act;
mod askpass;
mod changes;
mod read;
mod sweep;

pub(crate) use act::{run_act, Act, ActDone, ActOrder, Acted, Asker};
pub(crate) use askpass::{answer_git, AskpassPlan};
pub(crate) use changes::{capture_baseline, read_session, ChangesDone, SessionRead};
pub(crate) use read::read_roots;
pub(crate) use sweep::{check_candidates, remove_candidates, Candidate, Sweep, SweepDone};

use futures::channel::oneshot;
use krad_agents::Response;
use krad_domain::{EnvironmentName, GroupGit, Host};
use krad_protocol::{EventKind, GitRead};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use self::read::PollPace;
use crate::workers::LeadRemovals;

pub(crate) type GitKey = (Host, PathBuf);

#[derive(Debug)]
pub(crate) struct ReadDone {
    pub(crate) environment: EnvironmentName,
    pub(crate) host: Host,
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) reads: HashMap<PathBuf, GroupGit>,
}

#[derive(Debug)]
pub(crate) struct BusAnswer {
    pub(crate) reply: oneshot::Sender<Response>,
    pub(crate) response: Response,
}

#[derive(Debug)]
pub(crate) enum GitJob {
    Read(ReadDone),
    Acted(ActDone),
    Sweep(SweepDone),
    Changes(ChangesDone),
    Removals(LeadRemovals),
    Answered(BusAnswer),
}

#[derive(Debug)]
struct Watch {
    pace: PollPace,
    due: Instant,
    reading: bool,
    again: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Git {
    reads: HashMap<GitKey, GroupGit>,
    watches: HashMap<EnvironmentName, Watch>,
    pub(crate) sweep: Sweep,
}

impl Git {
    pub(crate) fn starts_reading(
        &mut self,
        environment: &EnvironmentName,
        remote: bool,
        now: Instant,
    ) -> bool {
        let watch = self
            .watches
            .entry(environment.clone())
            .or_insert_with(|| Watch {
                pace: PollPace::new(remote),
                due: now,
                reading: false,
                again: false,
            });
        if watch.reading || now < watch.due {
            return false;
        }
        watch.reading = true;
        true
    }

    pub(crate) fn reading_failed(&mut self, environment: &EnvironmentName) {
        if let Some(watch) = self.watches.get_mut(environment) {
            watch.reading = false;
        }
    }

    pub(crate) fn refresh(&mut self, environment: &EnvironmentName) {
        let Some(watch) = self.watches.get_mut(environment) else {
            return;
        };
        if watch.reading {
            watch.again = true;
        } else {
            watch.due = Instant::now();
        }
    }

    pub(crate) fn keep_environments(&mut self, open: impl Fn(&EnvironmentName) -> bool) {
        self.watches.retain(|environment, _| open(environment));
    }

    pub(crate) fn read(
        &mut self,
        done: ReadDone,
        live: &HashSet<GitKey>,
        outbox: &mut Vec<EventKind>,
    ) -> Vec<PathBuf> {
        let ReadDone {
            environment,
            host,
            roots,
            mut reads,
        } = done;
        let mut moved = false;
        let mut status_moved = Vec::new();
        for root in roots {
            let key = (host.clone(), root);
            let now = reads.remove(&key.1);
            let before = self.reads.get(&key);
            if before == now.as_ref() {
                continue;
            }
            moved = true;
            if let (Some(before), Some(now)) = (before, &now) {
                if before.status != now.status {
                    status_moved.push(key.1.clone());
                }
            }
            outbox.push(EventKind::GitRead(GitRead {
                host: key.0.clone(),
                root: key.1.clone(),
                git: now.clone().map(Box::new),
            }));
            match now {
                Some(now) => self.reads.insert(key, now),
                None => self.reads.remove(&key),
            };
        }
        self.prune(&host, live, outbox);
        if let Some(watch) = self.watches.get_mut(&environment) {
            let remote = !host.is_local();
            watch.pace = watch.pace.after(moved, remote);
            let wait = if watch.again {
                Duration::ZERO
            } else {
                watch.pace.current
            };
            watch.due = Instant::now() + wait;
            watch.reading = false;
            watch.again = false;
        }
        status_moved
    }

    fn prune(&mut self, host: &Host, live: &HashSet<GitKey>, outbox: &mut Vec<EventKind>) {
        let stale: Vec<GitKey> = self
            .reads
            .keys()
            .filter(|key| key.0 == *host && !live.contains(*key))
            .cloned()
            .collect();
        for (host, root) in stale {
            self.reads.remove(&(host.clone(), root.clone()));
            outbox.push(EventKind::GitRead(GitRead {
                host,
                root,
                git: None,
            }));
        }
    }

    pub(crate) fn at(&self, host: &Host, root: &Path) -> Option<&GroupGit> {
        self.reads.get(&(host.clone(), root.to_path_buf()))
    }

    pub(crate) fn snapshot(&self) -> Vec<GitRead> {
        self.reads
            .iter()
            .map(|((host, root), git)| GitRead {
                host: host.clone(),
                root: root.clone(),
                git: Some(Box::new(git.clone())),
            })
            .collect()
    }
}
