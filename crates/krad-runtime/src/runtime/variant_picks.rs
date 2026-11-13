use futures::channel::oneshot;
use krad_agents::{ErrorCode as BusCode, Response};
use krad_domain::{
    AppEvent, EnvironmentName, VariantId, VariantRemoval, VariantRunId, VariantWorktree,
    WinnerOutcome,
};
use krad_protocol::{ErrorCode, Reply};
use krad_repo::{apply_variant, discard_variant};
use std::path::PathBuf;

use super::working::agent;
use super::{refused, Runtime};
use crate::jobs::JobDone;
use crate::variants::{PickAsked, VariantJob};

const NO_RUN_TEXT: &str = "No variant run with that id in this environment";
const ALREADY_DECIDED_TEXT: &str = "This run already has a winner";
const JOB_NOT_STARTED: &str = "the variant's work could not start";

fn tell(reply: Option<oneshot::Sender<Response>>, response: Response) {
    if let Some(reply) = reply {
        if reply.send(response).is_err() {
            tracing::trace!("a bus caller left before its variant answer");
        }
    }
}

impl Runtime {
    pub(super) fn pick_variant(
        &mut self,
        environment: &EnvironmentName,
        (run, winner, outcome): (VariantRunId, VariantId, WinnerOutcome),
        removal: VariantRemoval,
        reply: Option<oneshot::Sender<Response>>,
    ) -> Result<(), Reply> {
        let mut planned = Err(refused(ErrorCode::NotFound, NO_RUN_TEXT));
        self.environments.variant_runs(
            environment,
            |runs| {
                let Some(found) = runs.run_mut(run) else {
                    return false;
                };
                if found.decision.is_some() || found.is_picking() {
                    planned = Err(refused(ErrorCode::AlreadyAnswered, ALREADY_DECIDED_TEXT));
                    return false;
                }
                planned = found
                    .begin_pick(winner, outcome)
                    .map_err(|error| refused(ErrorCode::Invalid, &error.to_string()));
                planned.is_ok()
            },
            &mut self.outbox,
        );
        let plan = match planned {
            Ok(plan) => plan,
            Err(refusal) => {
                if let Reply::Refused { message, .. } = &refusal {
                    tell(reply, Response::error(BusCode::BadRequest, message.clone()));
                }
                return Err(refusal);
            }
        };
        let pick = PickAsked {
            environment: environment.clone(),
            plan,
            removal,
            reply,
        };
        let Some(apply) = pick.plan.apply.clone() else {
            self.carry_out_pick(pick);
            return Ok(());
        };
        let Some(host) = self.environments.host_of(environment) else {
            return Err(refused(ErrorCode::NotFound, NO_RUN_TEXT));
        };
        let link = self.hosts.link(&host);
        let started = self.jobs.run(move || {
            let applied = apply_variant(&link, &apply.onto, &apply.from)
                .map(drop)
                .map_err(|error| error.to_string());
            JobDone::Variant(VariantJob::Applied { pick, applied })
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not start applying a variant");
            return Err(refused(ErrorCode::Unavailable, JOB_NOT_STARTED));
        }
        Ok(())
    }

    pub(super) fn variant_applied(&mut self, pick: PickAsked, applied: Result<(), String>) {
        match applied {
            Ok(()) => self.carry_out_pick(pick),
            Err(error) => {
                let run = pick.plan.run;
                self.environments.variant_runs(
                    &pick.environment,
                    |runs| {
                        runs.run_mut(run).map(krad_domain::VariantRun::abandon_pick);
                        true
                    },
                    &mut self.outbox,
                );
                tracing::warn!(run = run.0, "a variant could not be applied");
                tell(pick.reply, Response::error(BusCode::Failed, error));
            }
        }
    }

    fn carry_out_pick(&mut self, pick: PickAsked) {
        let PickAsked {
            environment,
            plan,
            removal,
            reply,
        } = pick;
        let mut base = None;
        self.environments.variant_runs(
            &environment,
            |runs| {
                let Some(found) = runs.run_mut(plan.run) else {
                    return false;
                };
                found.record_pick(&plan);
                base = Some((found.base_root.clone(), found.group));
                true
            },
            &mut self.outbox,
        );
        let Some((base_root, base_group)) = base else {
            return tell(reply, Response::error(BusCode::NotFound, NO_RUN_TEXT));
        };
        let mut worktrees = Vec::new();
        for step in &plan.discard {
            if let Some(terminal) = step.terminal {
                self.close_worker(&agent(&environment, terminal), false);
            }
            if let Some(VariantWorktree {
                path,
                branch,
                group,
            }) = step.worktree.clone()
            {
                self.remove_group(&environment, group);
                worktrees.push((path, branch));
            }
        }
        self.discard_variant_worktrees(&environment, base_root, worktrees, removal);
        let chosen = AppEvent::VariantWinnerChosen {
            run: plan.run,
            variant: plan.winner,
            outcome: plan.outcome,
        };
        self.events
            .publish_own(&environment, Some(base_group), &chosen, &mut self.outbox);
        tracing::info!(
            run = plan.run.0,
            winner = plan.winner.0,
            "variant winner chosen"
        );
        tell(
            reply,
            Response::VariantPicked {
                run: plan.run.0,
                variant: plan.winner.0,
            },
        );
    }

    pub(super) fn discard_variant_worktrees(
        &mut self,
        environment: &EnvironmentName,
        repo_root: PathBuf,
        worktrees: Vec<(PathBuf, String)>,
        removal: VariantRemoval,
    ) {
        if worktrees.is_empty() {
            return;
        }
        let Some(host) = self.environments.host_of(environment) else {
            return;
        };
        let link = self.hosts.link(&host);
        let environment = environment.clone();
        let started = self.jobs.run(move || {
            let errors = worktrees
                .iter()
                .filter_map(|(path, branch)| {
                    discard_variant(&link, &repo_root, path, branch, removal)
                        .err()
                        .map(|error| error.to_string())
                })
                .collect();
            JobDone::Variant(VariantJob::Removed {
                environment,
                errors,
            })
        });
        if let Err(error) = started {
            tracing::warn!(%error, "could not start removing variant worktrees");
        }
    }

    pub(super) fn variant_worktrees_removed(
        &mut self,
        environment: &EnvironmentName,
        errors: &[String],
    ) {
        for error in errors {
            tracing::warn!(%error, "variant worktree not removed");
        }
        self.events.publish_own(
            environment,
            None,
            &AppEvent::WorktreeRemoved,
            &mut self.outbox,
        );
    }
}
