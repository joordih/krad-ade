use krad_agents::Caller;
use krad_domain::{parse_wsl_path, AppEvent, EnvironmentName, GroupId, Host, WorktreeState};
use krad_protocol::{EventKind, WorktreeSwept};
use krad_repo::{Askpass, RepoError};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::working::reply_with;
use super::Runtime;
use crate::git::{
    check_candidates, read_roots, remove_candidates, AskpassPlan, BusAnswer, Candidate, GitJob,
    GitKey, ReadDone, SweepDone,
};
use crate::hosts::{remote_askpass, Reach};
use crate::jobs::JobDone;

impl Runtime {
    pub(super) fn tick_git(&mut self) {
        let environments = &self.environments;
        self.git.keep_environments(|name| environments.knows(name));
        let now = Instant::now();
        let watched: Vec<(EnvironmentName, Host, Vec<PathBuf>)> = self
            .environments
            .iter()
            .map(|data| {
                let roots = data.groups.iter().map(|group| group.root.clone());
                (data.name.clone(), data.host.clone(), roots.collect())
            })
            .collect();
        for (environment, host, roots) in watched {
            let link = self.hosts.link(&host);
            if !link.is_ready() || !self.git.starts_reading(&environment, !host.is_local(), now) {
                continue;
            }
            let reading = environment.clone();
            let started = self.jobs.run(move || {
                let reads = read_roots(&link, roots.clone());
                JobDone::Git(GitJob::Read(ReadDone {
                    environment: reading,
                    host,
                    roots,
                    reads,
                }))
            });
            if let Err(error) = started {
                tracing::warn!(%error, "could not start reading git");
                self.git.reading_failed(&environment);
            }
        }
    }

    pub(super) fn git_job_done(&mut self, job: GitJob) {
        match job {
            GitJob::Read(done) => self.git_read(done),
            GitJob::Acted(done) => self.git_acted(done),
            GitJob::Sweep(done) => self.sweep_done(done),
            GitJob::Changes(done) => self.changes_done(done),
            GitJob::Removals(done) => self.removals_done(done),
            GitJob::Answered(BusAnswer { reply, response }) => reply_with(reply, response),
        }
    }

    fn live_roots(&self) -> HashSet<GitKey> {
        self.environments
            .iter()
            .flat_map(|data| {
                data.groups
                    .iter()
                    .map(|group| (data.host.clone(), group.root.clone()))
            })
            .collect()
    }

    fn git_read(&mut self, done: ReadDone) {
        let (environment, host) = (done.environment.clone(), done.host.clone());
        let live = self.live_roots();
        let moved = self.git.read(done, &live, &mut self.outbox);
        let changed: Vec<(EnvironmentName, GroupId)> = self
            .environments
            .iter()
            .filter(|data| data.host == host)
            .flat_map(|data| {
                data.groups
                    .iter()
                    .filter(|group| moved.contains(&group.root))
                    .map(|group| (data.name.clone(), group.id))
            })
            .collect();
        for (name, group) in changed {
            self.events
                .publish_own(&name, Some(group), &AppEvent::GitChanged, &mut self.outbox);
        }
        self.sweep_worktrees(&environment);
    }

    fn sweep_worktrees(&mut self, environment: &EnvironmentName) {
        let Some(data) = self.environments.data(environment) else {
            return;
        };
        let settings = self.settings.current.worktrees.clone();
        let candidates = self.git.sweep_candidates(data, &settings);
        if candidates.is_empty() {
            return;
        }
        let link = self.hosts.link(&data.host);
        let checking = environment.clone();
        let started = self.jobs.run(move || {
            let checked = check_candidates(&link, &settings, candidates);
            JobDone::Git(GitJob::Sweep(SweepDone::Checked {
                environment: checking,
                checked,
            }))
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not check merged worktrees");
            self.git.sweep.finish();
        }
    }

    fn sweep_done(&mut self, done: SweepDone) {
        match done {
            SweepDone::Checked {
                environment,
                checked,
            } => {
                let Some(data) = self.environments.data(&environment) else {
                    self.git.sweep.finish();
                    return;
                };
                let settings = self.settings.current.worktrees.clone();
                let removable = self.git.sweep.removable(data, settings.cleanup, checked);
                if removable.is_empty() {
                    return;
                }
                let link = self.hosts.link(&data.host);
                let started = self.jobs.run(move || {
                    let removed = remove_candidates(&link, &settings, removable);
                    JobDone::Git(GitJob::Sweep(SweepDone::Removed {
                        environment,
                        removed,
                    }))
                });
                if let Err(error) = started {
                    tracing::warn!(%error, "could not remove merged worktrees");
                    self.git.sweep.finish();
                }
            }
            SweepDone::Removed {
                environment,
                removed,
            } => self.swept(&environment, removed),
        }
    }

    fn swept(
        &mut self,
        environment: &EnvironmentName,
        removed: Vec<(Candidate, Result<WorktreeState, RepoError>)>,
    ) {
        self.git.sweep.finish();
        for (candidate, outcome) in removed {
            match outcome {
                Ok(WorktreeState::Merged) => {
                    tracing::info!(group = candidate.group.0, "merged worktree removed");
                    self.git.sweep.removed(environment, candidate.group);
                    self.events.publish_own(
                        environment,
                        Some(candidate.group),
                        &AppEvent::WorktreeRemoved,
                        &mut self.outbox,
                    );
                    self.remove_group(environment, candidate.group);
                    self.outbox.push(EventKind::WorktreeSwept(WorktreeSwept {
                        environment: environment.clone(),
                        group: candidate.group,
                        name: candidate.name,
                    }));
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "merged worktree not removed"),
            }
        }
        self.git.refresh(environment);
    }

    pub(super) fn git_askpass(
        &self,
        environment: &EnvironmentName,
        host: &Host,
        root: &Path,
    ) -> AskpassPlan {
        let Some(access) = self.bus.as_ref().map(crate::bus::Bus::access) else {
            return AskpassPlan::Off;
        };
        let caller = Caller::Git(environment.as_str().to_owned());
        if !host.is_local() {
            return match self.hosts.reach(environment, host) {
                Reach::Ready(Some(remote)) => {
                    let (program, vars) = remote_askpass(remote, &caller);
                    AskpassPlan::Ready(Askpass::new(program, vars))
                }
                Reach::Ready(None) | Reach::BusPreparing | Reach::Unreached => AskpassPlan::Off,
            };
        }
        if parse_wsl_path(root).is_some() {
            return AskpassPlan::Off;
        }
        AskpassPlan::local(access, &caller)
    }
}
