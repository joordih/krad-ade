use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    AppEvent, EnvironmentName, TerminalId, VariantEvent, VariantId, VariantRunId, VariantWorktree,
};
use krad_repo::{run_task, variant_diff_summary, TASK_RUN_TIMEOUT};
use std::path::PathBuf;
use std::time::SystemTime;

use super::super::EnvironmentShell;

const READY_PREFIX: &str = "Variants of run ";
const READY_SUFFIX: &str = " are ready to compare";

struct Finishing {
    run: VariantRunId,
    variant: VariantId,
    base_root: PathBuf,
    worktree: VariantWorktree,
    test: Option<krad_domain::ProjectTask>,
}

impl EnvironmentShell {
    pub(super) fn apply_variant_event(
        &mut self,
        env: &EnvironmentName,
        run: VariantRunId,
        variant: VariantId,
        event: VariantEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(found) = self.variant_run_mut(env, run) else {
            return false;
        };
        let applied = found.variant_mut(variant).map(|target| target.apply(event));
        match applied {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                tracing::debug!(%error, run = run.0, variant = variant.0, "variant event ignored");
                return false;
            }
            None => return false,
        }
        let finished = found.take_finished_announcement();
        let group = found.group;
        if finished {
            self.publish_event(env, Some(group), AppEvent::VariantsFinished { run });
            self.push_toast(format!("{READY_PREFIX}{run}{READY_SUFFIX}"));
        }
        self.store();
        cx.notify();
        true
    }

    pub(in crate::environment_shell) fn note_variant_progress(
        &mut self,
        env: &EnvironmentName,
        event: &AppEvent,
    ) {
        let terminal = match event {
            AppEvent::AgentStarted { terminal, .. } | AppEvent::AgentFinished { terminal } => {
                *terminal
            }
            _ => return,
        };
        let owned = self
            .envs
            .get(env)
            .is_some_and(|live| live.data.variant_runs.of_terminal(terminal).is_some());
        if owned {
            self.variants.progress.push((env.clone(), event.clone()));
        }
    }

    pub(in crate::environment_shell) fn flush_variant_progress(&mut self, cx: &mut Context<Self>) {
        for (env, event) in std::mem::take(&mut self.variants.progress) {
            self.note_variant_agent(&env, &event, cx);
        }
    }

    fn note_variant_agent(
        &mut self,
        env: &EnvironmentName,
        event: &AppEvent,
        cx: &mut Context<Self>,
    ) {
        let (terminal, change) = match event {
            AppEvent::AgentStarted { terminal, .. } => (*terminal, VariantEvent::Working),
            AppEvent::AgentFinished { terminal } => (
                *terminal,
                VariantEvent::Finished {
                    at: SystemTime::now(),
                },
            ),
            _ => return,
        };
        let Some((run, variant)) = self
            .envs
            .get(env)
            .and_then(|live| live.data.variant_runs.of_terminal(terminal))
        else {
            return;
        };
        let finished = matches!(change, VariantEvent::Finished { .. });
        if self.apply_variant_event(env, run, variant, change, cx) && finished {
            self.finish_variant(env, run, variant, cx);
        }
    }

    pub(in crate::environment_shell) fn note_variant_spend(
        &mut self,
        env: &EnvironmentName,
        chat: TerminalId,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        let spent = live.data.chat_spend(chat);
        let Some((run, variant)) = live.data.variant_runs.of_terminal(chat) else {
            return;
        };
        if spent.is_empty() {
            return;
        }
        if let Some(target) = live
            .data
            .variant_runs
            .run_mut(run)
            .and_then(|found| found.variant_mut(variant))
        {
            target.spent = spent;
        }
    }

    fn finish_variant(
        &mut self,
        env: &EnvironmentName,
        run: VariantRunId,
        variant: VariantId,
        cx: &mut Context<Self>,
    ) {
        let Some(finishing) = self.variant_run(env, run).and_then(|found| {
            Some(Finishing {
                run,
                variant,
                base_root: found.base_root.clone(),
                worktree: found.variant(variant)?.worktree.clone()?,
                test: found.test.clone(),
            })
        }) else {
            return;
        };
        self.summarize_variant(env, &finishing, cx);
        if finishing.test.is_some() {
            self.test_variant(env, finishing, cx);
        }
    }

    fn summarize_variant(
        &self,
        env: &EnvironmentName,
        finishing: &Finishing,
        cx: &mut Context<Self>,
    ) {
        let env = env.clone();
        let link = self.env_link(&env);
        let base = finishing.base_root.clone();
        let root = finishing.worktree.path.clone();
        let (run, variant) = (finishing.run, finishing.variant);
        let summary = cx.background_spawn(async move { variant_diff_summary(&link, &base, &root) });
        cx.spawn(async move |this, cx| {
            let summary = summary.await;
            this.update(cx, |shell, cx| {
                let summary = match summary {
                    Ok(summary) => summary,
                    Err(error) => {
                        tracing::warn!(%error, run = run.0, "variant diff unreadable");
                        return;
                    }
                };
                if let Some(target) = shell
                    .variant_run_mut(&env, run)
                    .and_then(|found| found.variant_mut(variant))
                {
                    target.diff = Some(summary);
                }
                shell.store();
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn test_variant(
        &mut self,
        env: &EnvironmentName,
        finishing: Finishing,
        cx: &mut Context<Self>,
    ) {
        let Finishing {
            run,
            variant,
            worktree,
            test: Some(task),
            ..
        } = finishing
        else {
            return;
        };
        if !self.apply_variant_event(env, run, variant, VariantEvent::TestsStarted, cx) {
            return;
        }
        let env = env.clone();
        let link = self.env_link(&env);
        let outcome =
            cx.background_spawn(
                async move { run_task(&link, &worktree.path, &task, TASK_RUN_TIMEOUT) },
            );
        cx.spawn(async move |this, cx| {
            let outcome = outcome.await;
            this.update(cx, |shell, cx| {
                tracing::info!(run = run.0, variant = variant.0, verdict = ?outcome.verdict, "variant tested");
                shell.apply_variant_event(&env, run, variant, VariantEvent::TestsDone(outcome), cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn retest_variant(
        &mut self,
        env: &EnvironmentName,
        run: VariantRunId,
        variant: VariantId,
        cx: &mut Context<Self>,
    ) {
        self.finish_variant(env, run, variant, cx);
    }

    pub(super) fn check_variant_worktrees(&self, env: &EnvironmentName, cx: &mut Context<Self>) {
        let Some(paths) = self.envs.get(env).map(|live| {
            live.data
                .variant_runs
                .all()
                .iter()
                .flat_map(|run| &run.variants)
                .filter(|variant| !variant.status.is_closed())
                .filter_map(|variant| variant.worktree.as_ref().map(|tree| tree.path.clone()))
                .collect::<Vec<PathBuf>>()
        }) else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        let env = env.clone();
        let link = self.env_link(&env);
        let checked = cx.background_spawn(async move {
            paths
                .into_iter()
                .filter(|path| !link.exists(path))
                .collect::<Vec<PathBuf>>()
        });
        cx.spawn(async move |this, cx| {
            let gone = checked.await;
            if gone.is_empty() {
                return;
            }
            this.update(cx, |shell, cx| {
                let Some(live) = shell.envs.get_mut(&env) else {
                    return;
                };
                let missing = live
                    .data
                    .variant_runs
                    .mark_missing_worktrees(|path| !gone.iter().any(|gone| gone == path));
                tracing::info!(count = missing.len(), "variant worktrees missing");
                let finished: Vec<(VariantRunId, krad_domain::GroupId)> = live
                    .data
                    .variant_runs
                    .all()
                    .iter()
                    .map(|run| (run.id, run.group))
                    .collect();
                for (run, group) in finished {
                    let announce = shell
                        .variant_run_mut(&env, run)
                        .is_some_and(krad_domain::VariantRun::take_finished_announcement);
                    if announce {
                        shell.publish_event(&env, Some(group), AppEvent::VariantsFinished { run });
                    }
                }
                shell.store();
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }
}
