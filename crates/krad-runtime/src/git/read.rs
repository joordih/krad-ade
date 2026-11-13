use krad_domain::GroupGit;
use krad_host::HostLink;
use krad_repo::{line_counts, list_refs, list_worktrees, repo_root, status};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const LOCAL_POLL: Duration = Duration::from_secs(3);
const REMOTE_POLL: Duration = Duration::from_secs(5);
const SLOWEST_POLL: Duration = Duration::from_secs(20);
const QUIET_GROWTH: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PollPace {
    fastest: Duration,
    pub(crate) current: Duration,
}

impl PollPace {
    pub(crate) const fn new(remote: bool) -> Self {
        let fastest = if remote { REMOTE_POLL } else { LOCAL_POLL };
        Self {
            fastest,
            current: fastest,
        }
    }

    pub(crate) fn after(self, changed: bool, remote: bool) -> Self {
        let start = Self::new(remote);
        if changed || start.fastest != self.fastest {
            return start;
        }
        Self {
            current: self.current.saturating_mul(QUIET_GROWTH).min(SLOWEST_POLL),
            ..self
        }
    }
}

fn read_root(link: &HostLink, root: &Path) -> Option<GroupGit> {
    let repo_root = repo_root(link, root).ok().flatten()?;
    std::thread::scope(|scope| {
        let lines = scope.spawn(|| line_counts(link, &repo_root).unwrap_or_default());
        let worktrees = scope.spawn(|| list_worktrees(link, root).unwrap_or_default());
        let refs = scope.spawn(|| list_refs(link, root).unwrap_or_default());
        let status = status(link, root).ok();
        let lines = lines.join().unwrap_or_default();
        let worktrees = worktrees.join().unwrap_or_default();
        let refs = refs.join().unwrap_or_default();
        Some(GroupGit {
            status: status?,
            repo_root: repo_root.clone(),
            lines,
            worktrees,
            refs,
        })
    })
}

#[tracing::instrument(skip_all, fields(roots = roots.len()))]
pub(crate) fn read_roots(link: &HostLink, roots: Vec<PathBuf>) -> HashMap<PathBuf, GroupGit> {
    std::thread::scope(|scope| {
        let reads: Vec<_> = roots
            .into_iter()
            .map(|root| scope.spawn(move || Some((root.clone(), read_root(link, &root)?))))
            .collect();
        reads
            .into_iter()
            .filter_map(|read| read.join().ok().flatten())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_repository_is_polled_less_often_until_it_changes() {
        let start = PollPace::new(true);
        assert_eq!(start.current, REMOTE_POLL);
        let quiet = start.after(false, true).after(false, true);
        assert_eq!(quiet.current, REMOTE_POLL * 4);
        let slowest = (0..10).fold(quiet, |pace, _| pace.after(false, true));
        assert_eq!(slowest.current, SLOWEST_POLL);
        assert_eq!(slowest.after(true, true).current, REMOTE_POLL);
        assert_eq!(slowest.after(false, false).current, LOCAL_POLL);
    }
}
