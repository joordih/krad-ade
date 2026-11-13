use krad_domain::{
    removes_automatically, AgentRun, EnvironmentData, EnvironmentName, GroupGit, GroupId,
    WorktreeCleanup, WorktreeOccupancy, WorktreeSettings, WorktreeState,
};
use krad_host::HostLink;
use krad_repo::{remove_merged_worktree, worktree_state, RepoError};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::Git;

type SweepKey = (EnvironmentName, GroupId);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
    occupancy: WorktreeOccupancy,
    worktree: Option<GroupGit>,
    repo: Option<GroupGit>,
}

#[derive(Debug, Default)]
pub(crate) struct Sweep {
    checked: HashMap<SweepKey, Fingerprint>,
    finished: HashSet<SweepKey>,
    running: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) group: GroupId,
    pub(crate) name: String,
    repo_root: PathBuf,
    path: PathBuf,
    branch: String,
}

#[derive(Debug)]
pub(crate) enum SweepDone {
    Checked {
        environment: EnvironmentName,
        checked: Vec<(Candidate, Result<WorktreeState, RepoError>)>,
    },
    Removed {
        environment: EnvironmentName,
        removed: Vec<(Candidate, Result<WorktreeState, RepoError>)>,
    },
}

fn occupancy(data: &EnvironmentData, group: GroupId) -> WorktreeOccupancy {
    WorktreeOccupancy::of(data.nodes.iter().filter(|node| node.group == Some(group)))
}

impl Sweep {
    pub(crate) fn note_finished(&mut self, environment: &EnvironmentName, group: GroupId) {
        self.finished.insert((environment.clone(), group));
    }

    fn agent_run(&self, environment: &EnvironmentName, group: GroupId) -> AgentRun {
        if self.finished.contains(&(environment.clone(), group)) {
            AgentRun::Finished
        } else {
            AgentRun::NotSeen
        }
    }

    pub(crate) fn removable(
        &mut self,
        data: &EnvironmentData,
        cleanup: WorktreeCleanup,
        checked: Vec<(Candidate, Result<WorktreeState, RepoError>)>,
    ) -> Vec<Candidate> {
        let removable: Vec<Candidate> = checked
            .into_iter()
            .filter_map(|(candidate, state)| {
                let state = state
                    .inspect_err(|error| tracing::debug!(%error, "worktree state unreadable"))
                    .ok()?;
                let run = self.agent_run(&data.name, candidate.group);
                let occupancy = occupancy(data, candidate.group);
                removes_automatically(cleanup, run, occupancy, state).then_some(candidate)
            })
            .collect();
        if removable.is_empty() {
            self.running = false;
        }
        removable
    }

    pub(crate) fn removed(&mut self, environment: &EnvironmentName, group: GroupId) {
        let key = (environment.clone(), group);
        self.finished.remove(&key);
        self.checked.remove(&key);
    }

    pub(crate) const fn finish(&mut self) {
        self.running = false;
    }
}

impl Git {
    pub(crate) fn sweep_candidates(
        &mut self,
        data: &EnvironmentData,
        settings: &WorktreeSettings,
    ) -> Vec<Candidate> {
        if settings.cleanup != WorktreeCleanup::RemoveWhenMerged || self.sweep.running {
            return Vec::new();
        }
        let environment = &data.name;
        let mut seen = Vec::new();
        let mut candidates = Vec::new();
        for group in &data.groups {
            let Some(origin) = &group.worktree else {
                continue;
            };
            if self.sweep.agent_run(environment, group.id) == AgentRun::NotSeen {
                continue;
            }
            let occupancy = occupancy(data, group.id);
            if occupancy == WorktreeOccupancy::InUse {
                continue;
            }
            let worktree = self.at(&data.host, &group.root);
            if worktree.is_some_and(|git| !git.status.changes.is_empty()) {
                continue;
            }
            let key = (environment.clone(), group.id);
            let fingerprint = Fingerprint {
                occupancy,
                worktree: worktree.cloned(),
                repo: self.at(&data.host, &origin.repo_root).cloned(),
            };
            if self.sweep.checked.get(&key) == Some(&fingerprint) {
                continue;
            }
            seen.push((key, fingerprint));
            candidates.push(Candidate {
                group: group.id,
                name: group.name.clone(),
                repo_root: origin.repo_root.clone(),
                path: group.root.clone(),
                branch: origin.branch.clone(),
            });
        }
        self.sweep.checked.extend(seen);
        self.sweep.running = !candidates.is_empty();
        candidates
    }
}

#[tracing::instrument(skip_all, fields(candidates = candidates.len()))]
pub(crate) fn check_candidates(
    link: &HostLink,
    settings: &WorktreeSettings,
    candidates: Vec<Candidate>,
) -> Vec<(Candidate, Result<WorktreeState, RepoError>)> {
    candidates
        .into_iter()
        .map(|candidate| {
            let state = worktree_state(
                link,
                &candidate.repo_root,
                &candidate.path,
                Some(&candidate.branch),
                settings.base,
            );
            (candidate, state)
        })
        .collect()
}

#[tracing::instrument(skip_all, fields(candidates = candidates.len()))]
pub(crate) fn remove_candidates(
    link: &HostLink,
    settings: &WorktreeSettings,
    candidates: Vec<Candidate>,
) -> Vec<(Candidate, Result<WorktreeState, RepoError>)> {
    candidates
        .into_iter()
        .map(|candidate| {
            let removed = remove_merged_worktree(
                link,
                &candidate.repo_root,
                &candidate.path,
                Some(&candidate.branch),
                settings.base,
            );
            (candidate, removed)
        })
        .collect()
}
