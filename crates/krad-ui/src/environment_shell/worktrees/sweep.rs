use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    removes_automatically, AgentRun, AppEvent, EnvironmentName, GroupId, ToastKind,
    WorktreeCleanup, WorktreeOccupancy, WorktreeState,
};
use krad_repo::{remove_merged_worktree, worktree_state, RepoError};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::super::groups::GroupGit;
use super::super::EnvironmentShell;

const REMOVED_PREFIX: &str = "Removed the merged worktree ";

type SweepKey = (EnvironmentName, GroupId);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
    occupancy: WorktreeOccupancy,
    worktree: Option<GroupGit>,
    repo: Option<GroupGit>,
}

#[derive(Default)]
pub(in crate::environment_shell) struct WorktreeSweep {
    checked: HashMap<SweepKey, Fingerprint>,
    finished: HashSet<SweepKey>,
    running: bool,
}

#[derive(Clone, Debug)]
struct Candidate {
    group: GroupId,
    name: String,
    repo_root: PathBuf,
    path: PathBuf,
    branch: String,
}

type Checked = Vec<(Candidate, Result<WorktreeState, RepoError>)>;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn note_agent_finished_in(
        &mut self,
        env: &EnvironmentName,
        group: Option<GroupId>,
    ) {
        if let Some(group) = group {
            self.worktree_sweep.finished.insert((env.clone(), group));
        }
    }

    fn agent_run(&self, env: &EnvironmentName, group: GroupId) -> AgentRun {
        if self.worktree_sweep.finished.contains(&(env.clone(), group)) {
            AgentRun::Finished
        } else {
            AgentRun::NotSeen
        }
    }

    fn occupancy(&self, env: &EnvironmentName, group: GroupId) -> WorktreeOccupancy {
        self.envs.get(env).map_or(WorktreeOccupancy::InUse, |live| {
            WorktreeOccupancy::of(
                live.data
                    .nodes
                    .iter()
                    .filter(|node| node.group == Some(group)),
            )
        })
    }

    fn sweep_candidates(&mut self, env: &EnvironmentName) -> Vec<Candidate> {
        let Some(live) = self.envs.get(env) else {
            return Vec::new();
        };
        let mut seen: Vec<(SweepKey, Fingerprint)> = Vec::new();
        let mut candidates = Vec::new();
        for group in &live.data.groups {
            let Some(origin) = &group.worktree else {
                continue;
            };
            if self.agent_run(env, group.id) == AgentRun::NotSeen {
                continue;
            }
            let occupancy = self.occupancy(env, group.id);
            if occupancy == WorktreeOccupancy::InUse {
                continue;
            }
            let worktree = self.git_at(env, &group.root);
            if worktree.is_some_and(|git| !git.status.changes.is_empty()) {
                continue;
            }
            let key = (env.clone(), group.id);
            let fingerprint = Fingerprint {
                occupancy,
                worktree: worktree.cloned(),
                repo: self.git_at(env, &origin.repo_root).cloned(),
            };
            if self.worktree_sweep.checked.get(&key) == Some(&fingerprint) {
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
        self.worktree_sweep.checked.extend(seen);
        candidates
    }

    #[tracing::instrument(skip_all)]
    pub(in crate::environment_shell) fn sweep_merged_worktrees(&mut self, cx: &mut Context<Self>) {
        let cleanup = self.settings.worktrees.cleanup;
        if cleanup != WorktreeCleanup::RemoveWhenMerged || self.worktree_sweep.running {
            return;
        }
        let env = self.active.clone();
        let candidates = self.sweep_candidates(&env);
        if candidates.is_empty() {
            return;
        }
        self.worktree_sweep.running = true;
        let link = self.env_link(&env);
        let base = self.settings.worktrees.base;
        let checking = cx.background_spawn(async move {
            candidates
                .into_iter()
                .map(|candidate| {
                    let state = worktree_state(
                        &link,
                        &candidate.repo_root,
                        &candidate.path,
                        Some(&candidate.branch),
                        base,
                    );
                    (candidate, state)
                })
                .collect::<Checked>()
        });
        cx.spawn(async move |this, cx| {
            let checked = checking.await;
            this.update(cx, |shell, cx| shell.remove_swept(&env, checked, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn remove_swept(&mut self, env: &EnvironmentName, checked: Checked, cx: &mut Context<Self>) {
        let cleanup = self.settings.worktrees.cleanup;
        let removable: Vec<Candidate> = checked
            .into_iter()
            .filter_map(|(candidate, state)| {
                let state = state
                    .inspect_err(|error| tracing::debug!(%error, "worktree state unreadable"))
                    .ok()?;
                let run = self.agent_run(env, candidate.group);
                let occupancy = self.occupancy(env, candidate.group);
                removes_automatically(cleanup, run, occupancy, state).then_some(candidate)
            })
            .collect();
        if removable.is_empty() {
            self.worktree_sweep.running = false;
            return;
        }
        let link = self.env_link(env);
        let base = self.settings.worktrees.base;
        let removing = cx.background_spawn(async move {
            removable
                .into_iter()
                .map(|candidate| {
                    let removed = remove_merged_worktree(
                        &link,
                        &candidate.repo_root,
                        &candidate.path,
                        Some(&candidate.branch),
                        base,
                    );
                    (candidate, removed)
                })
                .collect::<Checked>()
        });
        let env = env.clone();
        cx.spawn(async move |this, cx| {
            let removed = removing.await;
            this.update(cx, |shell, cx| shell.finish_sweep(&env, removed, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    fn finish_sweep(&mut self, env: &EnvironmentName, removed: Checked, cx: &mut Context<Self>) {
        self.worktree_sweep.running = false;
        for (candidate, outcome) in removed {
            match outcome {
                Ok(WorktreeState::Merged) => {
                    tracing::info!(group = candidate.group.0, "merged worktree removed");
                    let key = (env.clone(), candidate.group);
                    self.worktree_sweep.finished.remove(&key);
                    self.worktree_sweep.checked.remove(&key);
                    self.publish_event(env, Some(candidate.group), AppEvent::WorktreeRemoved);
                    self.remove_group_in(env, candidate.group, cx);
                    self.push_toast(
                        ToastKind::Success,
                        format!("{REMOVED_PREFIX}{}", candidate.name),
                    );
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "merged worktree not removed"),
            }
        }
        self.refresh_git(cx);
        cx.notify();
    }
}
