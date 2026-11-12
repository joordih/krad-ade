use gpui_kit::{AppContext as _, Context};
use krad_agents::{ErrorCode, Response};
use krad_domain::{
    AppEvent, CleanupPlan, DiscardStep, EnvironmentName, GroupId, TerminalId, ToastKind, VariantId,
    VariantRunId, WinnerOutcome,
};
use krad_repo::{apply_variant, discard_variant, VariantRemoval};
use std::path::PathBuf;

use super::super::EnvironmentShell;
use super::{answer, VariantsReply};

const NO_RUN_TEXT: &str = "No variant run with that id in this environment";
const APPLIED_TEXT: &str = "applied onto the base worktree; review and commit it there";
const KEPT_TEXT: &str = "kept as its session";
const ALREADY_DECIDED_TEXT: &str = "This run already has a winner";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VariantPick {
    pub(super) run: VariantRunId,
    pub(super) winner: VariantId,
    pub(super) outcome: WinnerOutcome,
    pub(super) removal: VariantRemoval,
}

impl EnvironmentShell {
    pub(super) fn pick_variant(
        &mut self,
        env: &EnvironmentName,
        pick: VariantPick,
        reply: VariantsReply,
        cx: &mut Context<Self>,
    ) {
        let VariantPick {
            run,
            winner,
            outcome,
            removal,
        } = pick;
        let planned = self
            .variant_run_mut(env, run)
            .ok_or_else(|| NO_RUN_TEXT.to_owned())
            .and_then(|found| {
                found
                    .begin_pick(winner, outcome)
                    .map_err(|error| error.to_string())
            });
        let plan = match planned {
            Ok(plan) => plan,
            Err(error) => {
                answer(reply, Response::error(ErrorCode::BadRequest, error.clone()));
                return self.toast(ToastKind::Error, &error, cx);
            }
        };
        let Some(apply) = plan.apply.clone() else {
            return self.carry_out_pick(env, &plan, removal, reply, cx);
        };
        let env = env.clone();
        let link = self.env_link(&env);
        let applied =
            cx.background_spawn(async move { apply_variant(&link, &apply.onto, &apply.from) });
        cx.spawn(async move |this, cx| {
            let applied = applied.await;
            this.update(cx, |shell, cx| match applied {
                Ok(_) => shell.carry_out_pick(&env, &plan, removal, reply, cx),
                Err(error) => {
                    if let Some(found) = shell.variant_run_mut(&env, plan.run) {
                        found.abandon_pick();
                    }
                    let error = error.to_string();
                    answer(reply, Response::error(ErrorCode::Failed, error.clone()));
                    shell.toast(ToastKind::Error, &error, cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn carry_out_pick(
        &mut self,
        env: &EnvironmentName,
        plan: &CleanupPlan,
        removal: VariantRemoval,
        reply: VariantsReply,
        cx: &mut Context<Self>,
    ) {
        let Some(found) = self.variant_run_mut(env, plan.run) else {
            return answer(reply, Response::error(ErrorCode::NotFound, NO_RUN_TEXT));
        };
        if found.decision.is_some() {
            answer(
                reply,
                Response::error(ErrorCode::BadRequest, ALREADY_DECIDED_TEXT),
            );
            return self.toast(ToastKind::Warning, ALREADY_DECIDED_TEXT, cx);
        }
        found.record_pick(plan);
        let base_root = found.base_root.clone();
        let base_group = found.group;
        let winner = self
            .variant_run(env, plan.run)
            .map_or_default(|found| self.variant_label(found, plan.winner));
        for step in &plan.discard {
            self.discard_variant_session(env, step, cx);
        }
        self.remove_variant_worktrees(env, base_root, plan.discard.clone(), removal, cx);
        self.publish_event(
            env,
            Some(base_group),
            AppEvent::VariantWinnerChosen {
                run: plan.run,
                variant: plan.winner,
                outcome: plan.outcome,
            },
        );
        let how = match plan.outcome {
            WinnerOutcome::KeepWorktree => KEPT_TEXT,
            WinnerOutcome::ApplyOntoBase => APPLIED_TEXT,
        };
        self.push_toast(ToastKind::Success, format!("{winner} {how}"));
        tracing::info!(
            run = plan.run.0,
            winner = plan.winner.0,
            "variant winner chosen"
        );
        answer(
            reply,
            Response::VariantPicked {
                run: plan.run.0,
                variant: plan.winner.0,
            },
        );
        self.store();
        cx.notify();
    }

    fn discard_variant_session(
        &mut self,
        env: &EnvironmentName,
        step: &DiscardStep,
        cx: &mut Context<Self>,
    ) {
        let group = step.worktree.as_ref().map(|worktree| worktree.group);
        let held: Vec<TerminalId> = self.envs.get(env).map_or_default(|live| {
            live.data
                .nodes
                .iter()
                .filter(|node| group.is_some() && node.group == group)
                .map(|node| node.id)
                .chain(step.terminal)
                .collect()
        });
        for terminal in held {
            self.close_terminal_in(env, terminal, cx);
        }
        if let Some(group) = group {
            self.forget_variant_group(env, group);
        }
    }

    pub(super) fn forget_variant_group(&mut self, env: &EnvironmentName, group: GroupId) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let removed = live.data.remove_group(group);
        if live.selected_group == Some(group) {
            live.selected_group = live.data.groups.first().map(|group| group.id);
        }
        let host = live.data.host.clone();
        if let Some(removed) = removed {
            self.forget_group_toolchains(&host, &removed.root);
        }
    }

    fn remove_variant_worktrees(
        &self,
        env: &EnvironmentName,
        base_root: PathBuf,
        discard: Vec<DiscardStep>,
        removal: VariantRemoval,
        cx: &mut Context<Self>,
    ) {
        let worktrees: Vec<_> = discard
            .into_iter()
            .filter_map(|step| step.worktree)
            .collect();
        if worktrees.is_empty() {
            return;
        }
        let env = env.clone();
        let link = self.env_link(&env);
        let removed = cx.background_spawn(async move {
            worktrees
                .iter()
                .filter_map(|worktree| {
                    discard_variant(&link, &base_root, &worktree.path, &worktree.branch, removal)
                        .err()
                        .map(|error| error.to_string())
                })
                .collect::<Vec<String>>()
        });
        cx.spawn(async move |this, cx| {
            let errors = removed.await;
            this.update(cx, |shell, cx| {
                for error in &errors {
                    tracing::warn!(%error, "variant worktree not removed");
                    shell.push_toast(ToastKind::Error, error.clone());
                }
                shell.publish_event(&env, None, AppEvent::WorktreeRemoved);
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}
