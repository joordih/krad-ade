use krad_domain::{EnvironmentName, VariantEvent, VariantId, VariantRunId, VariantStatus};
use krad_repo::{run_task, variant_diff_summary, TASK_RUN_TIMEOUT};
use std::path::PathBuf;
use std::time::SystemTime;

use super::working::agent;
use super::Runtime;
use crate::chats::Launch;
use crate::jobs::JobDone;
use crate::variants::VariantJob;

impl Runtime {
    pub(super) fn follow_variants(&mut self) {
        let mut moves = Vec::new();
        for data in self.environments.iter() {
            for run in data.variant_runs.all() {
                for variant in run
                    .variants
                    .iter()
                    .filter(|variant| !variant.status.is_closed())
                {
                    let Some(terminal) = variant.terminal else {
                        continue;
                    };
                    let chat = agent(&data.name, terminal);
                    if !self.chats.is_open(&chat) || self.chats.launch(&chat) != Launch::Started {
                        continue;
                    }
                    let busy = self.chats.is_busy(&chat);
                    let event = match (&variant.status, busy) {
                        (VariantStatus::Working, false) => VariantEvent::Finished {
                            at: SystemTime::now(),
                        },
                        (VariantStatus::Finished | VariantStatus::Tested { .. }, true) => {
                            VariantEvent::Working
                        }
                        _ => continue,
                    };
                    moves.push((data.name.clone(), (run.id, variant.id), event));
                }
            }
        }
        for (environment, variant, event) in moves {
            let finished = matches!(event, VariantEvent::Finished { .. });
            if self.apply_variant_event(&environment, variant, event) && finished {
                self.finish_variant(&environment, variant.0, variant.1);
            }
        }
    }

    pub(super) fn finish_variant(
        &mut self,
        environment: &EnvironmentName,
        run: VariantRunId,
        variant: VariantId,
    ) {
        let Some(data) = self.environments.data(environment) else {
            return;
        };
        let Some((base_root, worktree, test)) = data.variant_runs.run(run).and_then(|found| {
            let worktree = found.variant(variant)?.worktree.clone()?;
            Some((found.base_root.clone(), worktree, found.test.clone()))
        }) else {
            return;
        };
        let link = self.hosts.link(&data.host);
        let summarized = {
            let (link, environment, root) =
                (link.clone(), environment.clone(), worktree.path.clone());
            self.jobs.run(move || {
                JobDone::Variant(VariantJob::Summarized {
                    summary: variant_diff_summary(&link, &base_root, &root)
                        .map_err(|error| error.to_string()),
                    environment,
                    variant: (run, variant),
                })
            })
        };
        if let Err(error) = summarized {
            tracing::warn!(%error, "could not summarize a variant");
        }
        let Some(task) = test else {
            return;
        };
        if !self.apply_variant_event(environment, (run, variant), VariantEvent::TestsStarted) {
            return;
        }
        let environment = environment.clone();
        let tested = self.jobs.run(move || {
            let outcome = run_task(&link, &worktree.path, &task, TASK_RUN_TIMEOUT);
            JobDone::Variant(VariantJob::Tested {
                environment,
                variant: (run, variant),
                outcome,
            })
        });
        if let Err(error) = tested {
            tracing::warn!(%error, "could not test a variant");
        }
    }

    pub(super) fn check_variant_worktrees(&mut self, environment: &EnvironmentName) {
        let Some(data) = self.environments.data(environment) else {
            return;
        };
        let paths: Vec<PathBuf> = data
            .variant_runs
            .all()
            .iter()
            .flat_map(|run| &run.variants)
            .filter(|variant| !variant.status.is_closed())
            .filter_map(|variant| variant.worktree.as_ref().map(|tree| tree.path.clone()))
            .collect();
        if paths.is_empty() {
            return;
        }
        let link = self.hosts.link(&data.host);
        let environment = environment.clone();
        let started = self.jobs.run(move || {
            let gone = paths
                .into_iter()
                .filter(|path| !link.exists(path))
                .collect();
            JobDone::Variant(VariantJob::Checked { environment, gone })
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not check variant worktrees");
        }
    }

    pub(super) fn variant_job_done(&mut self, job: VariantJob) {
        match job {
            VariantJob::WorktreeAdded { launch, created } => {
                self.variant_worktree_added(&launch, created);
            }
            VariantJob::Summarized {
                environment,
                variant: (run, variant),
                summary,
            } => {
                let summary = match summary {
                    Ok(summary) => summary,
                    Err(error) => {
                        return tracing::warn!(%error, run = run.0, "variant diff unreadable");
                    }
                };
                self.environments.variant_runs(
                    &environment,
                    |runs| {
                        let Some(target) = runs
                            .run_mut(run)
                            .and_then(|found| found.variant_mut(variant))
                        else {
                            return false;
                        };
                        target.diff = Some(summary);
                        true
                    },
                    &mut self.outbox,
                );
            }
            VariantJob::Tested {
                environment,
                variant,
                outcome,
            } => {
                tracing::info!(run = variant.0 .0, variant = variant.1 .0, verdict = ?outcome.verdict, "variant tested");
                self.apply_variant_event(&environment, variant, VariantEvent::TestsDone(outcome));
            }
            VariantJob::Checked { environment, gone } => {
                self.variant_worktrees_gone(&environment, &gone);
            }
            VariantJob::Applied { pick, applied } => self.variant_applied(pick, applied),
            VariantJob::Removed {
                environment,
                errors,
            } => self.variant_worktrees_removed(&environment, &errors),
            VariantJob::TasksRead { order, tasks } => self.variant_tasks_read(order, tasks),
        }
    }

    fn variant_worktrees_gone(&mut self, environment: &EnvironmentName, gone: &[PathBuf]) {
        if gone.is_empty() {
            return;
        }
        let mut finished = Vec::new();
        self.environments.variant_runs(
            environment,
            |runs| {
                let missing =
                    runs.mark_missing_worktrees(|path| !gone.iter().any(|gone| gone == path));
                tracing::info!(count = missing.len(), "variant worktrees missing");
                let ids: Vec<VariantRunId> = runs.all().iter().map(|run| run.id).collect();
                for id in ids {
                    let Some(run) = runs.run_mut(id) else {
                        continue;
                    };
                    if run.take_finished_announcement() {
                        finished.push((run.id, run.group));
                    }
                }
                !missing.is_empty() || !finished.is_empty()
            },
            &mut self.outbox,
        );
        for (run, group) in finished {
            let event = krad_domain::AppEvent::VariantsFinished { run };
            self.events
                .publish_own(environment, Some(group), &event, &mut self.outbox);
        }
    }
}
