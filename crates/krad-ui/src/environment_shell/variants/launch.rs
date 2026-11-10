use gpui_kit::{AppContext as _, Context};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AppEvent, EnvironmentName, GroupId, ModelId, Prompt, VariantEvent, VariantId, VariantRunId,
    VariantWorktree, VariantsOrder, WorkerSpawn, WorkerSurface, WorktreeOrigin,
};
use krad_repo::{add_worktree, discard_variant, VariantRemoval, Worktree};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::super::session_templates::ChatChoices;
use super::super::terminal_lifecycle::WorkerChat;
use super::super::EnvironmentShell;

const NOT_A_REPO_TEXT: &str = "Variants need a group inside a git repository";
const NOT_A_WORKER_TEXT: &str = "That agent cannot run as a variant";
const NO_CHAT_TEXT: &str = "The variant's chat could not open";
const SESSION_SEPARATOR: &str = " · ";

fn every_variant_is_a_worker(order: &VariantsOrder) -> bool {
    order
        .variants
        .iter()
        .all(|spec| spec.harness.worker_surface() == WorkerSurface::Chat)
}

struct VariantLaunch {
    run: VariantRunId,
    variant: VariantId,
    name: String,
}

impl EnvironmentShell {
    #[tracing::instrument(skip_all, fields(group = group.as_u64()))]
    pub(super) fn launch_variants(
        &mut self,
        env: &EnvironmentName,
        group: GroupId,
        order: VariantsOrder,
        cx: &mut Context<Self>,
    ) -> Result<VariantRunId, String> {
        let repo_root = self
            .envs
            .get(env)
            .and_then(|live| live.data.group(group))
            .and_then(|group| self.git_at(env, &group.root))
            .map(|git| git.repo_root.clone())
            .ok_or_else(|| NOT_A_REPO_TEXT.to_owned())?;
        if !every_variant_is_a_worker(&order) {
            return Err(NOT_A_WORKER_TEXT.to_owned());
        }
        let count = order.variants.len();
        let live = self
            .envs
            .get_mut(env)
            .ok_or_else(|| NOT_A_REPO_TEXT.to_owned())?;
        let run = live
            .data
            .variant_runs
            .start(group, repo_root.clone(), order, SystemTime::now())
            .map_err(|error| error.to_string())?;
        let launches: Vec<VariantLaunch> =
            live.data.variant_runs.run(run).map_or_default(|started| {
                started
                    .variants
                    .iter()
                    .map(|variant| VariantLaunch {
                        run,
                        variant: variant.id,
                        name: started.worktree_name(variant.id),
                    })
                    .collect()
            });
        tracing::info!(run = run.0, variants = count, "variants started");
        self.publish_event(
            env,
            Some(group),
            AppEvent::VariantsStarted {
                run,
                variants: count,
            },
        );
        self.create_variant_worktrees(env, repo_root, launches, cx);
        self.store();
        cx.notify();
        Ok(run)
    }

    fn create_variant_worktrees(
        &self,
        env: &EnvironmentName,
        repo_root: PathBuf,
        launches: Vec<VariantLaunch>,
        cx: &mut Context<Self>,
    ) {
        let env = env.clone();
        let link = self.env_link(&env);
        cx.spawn(async move |this, cx| {
            for launch in launches {
                let created = cx
                    .background_spawn({
                        let link = link.clone();
                        let repo_root = repo_root.clone();
                        let name = launch.name.clone();
                        async move { add_worktree(&link, &repo_root, &name) }
                    })
                    .await;
                let opened = this.update(cx, |shell, cx| match created {
                    Ok(worktree) => shell.open_variant(&env, &repo_root, &launch, worktree, cx),
                    Err(error) => shell.variant_failed(&env, &launch, &error.to_string(), cx),
                });
                if opened.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    fn variant_failed(
        &mut self,
        env: &EnvironmentName,
        launch: &VariantLaunch,
        error: &str,
        cx: &mut Context<Self>,
    ) {
        tracing::warn!(
            run = launch.run.0,
            variant = launch.variant.0,
            "variant could not start"
        );
        let event = VariantEvent::LaunchFailed(error.to_owned());
        self.apply_variant_event(env, launch.run, launch.variant, event, cx);
    }

    fn open_variant(
        &mut self,
        env: &EnvironmentName,
        repo_root: &std::path::Path,
        launch: &VariantLaunch,
        worktree: Worktree,
        cx: &mut Context<Self>,
    ) {
        let branch = worktree
            .branch
            .clone()
            .unwrap_or_else(|| launch.name.clone());
        let Some((spec, prompt)) = self.variant_run(env, launch.run).and_then(|run| {
            let variant = run.variant(launch.variant)?;
            Some((variant.spec.clone(), run.prompt.clone()))
        }) else {
            return self.roll_back_variant_worktree(
                env,
                repo_root,
                &worktree.path,
                &branch,
                None,
                cx,
            );
        };
        let origin = WorktreeOrigin {
            repo_root: repo_root.to_path_buf(),
            branch: branch.clone(),
        };
        let selected = self.envs.get(env).and_then(|live| live.selected_group);
        let group = self.add_group_to(env, worktree.path.clone(), Some(origin), cx);
        if let Some(live) = self.envs.get_mut(env) {
            live.selected_group = selected;
        }
        let Some(group) = group else {
            self.roll_back_variant_worktree(env, repo_root, &worktree.path, &branch, None, cx);
            return self.variant_failed(env, launch, NO_CHAT_TEXT, cx);
        };
        let harness = self.settings.acp.harness_name(spec.harness);
        let brief = VariantsOrder::brief(&prompt, &spec);
        let chat = WorkerChat {
            group: Some(group),
            preset: spec.harness,
            spawn: WorkerSpawn {
                lead: None,
                task: format!(
                    "{}{SESSION_SEPARATOR}{}",
                    launch.variant,
                    spec.label(&harness)
                ),
            },
            session: LiveSession::Start(new_session_id()),
            prompts: vec![Prompt::from(brief)],
            mode: None,
        };
        let terminal = self
            .alloc_terminal_id(env)
            .and_then(|id| self.open_worker_chat(env, id, chat, cx));
        let Some(terminal) = terminal else {
            self.roll_back_variant_worktree(
                env,
                repo_root,
                &worktree.path,
                &branch,
                Some(group),
                cx,
            );
            return self.variant_failed(env, launch, NO_CHAT_TEXT, cx);
        };
        self.choose_variant_model(env, terminal, spec.model);
        let event = VariantEvent::Launched {
            worktree: VariantWorktree {
                path: worktree.path,
                branch,
                group,
            },
            terminal,
            at: SystemTime::now(),
        };
        self.apply_variant_event(env, launch.run, launch.variant, event, cx);
    }

    fn roll_back_variant_worktree(
        &mut self,
        env: &EnvironmentName,
        repo_root: &Path,
        worktree: &Path,
        branch: &str,
        group: Option<GroupId>,
        cx: &mut Context<Self>,
    ) {
        if let Some(group) = group {
            self.forget_variant_group(env, group);
        }
        let link = self.env_link(env);
        let (repo_root, worktree, branch) = (
            repo_root.to_path_buf(),
            worktree.to_path_buf(),
            branch.to_owned(),
        );
        cx.background_spawn(async move {
            let removed = discard_variant(
                &link,
                &repo_root,
                &worktree,
                &branch,
                VariantRemoval::Forced,
            );
            if let Err(error) = removed {
                tracing::warn!(%error, "variant worktree not rolled back");
            }
        })
        .detach();
    }

    fn choose_variant_model(
        &mut self,
        env: &EnvironmentName,
        terminal: krad_domain::TerminalId,
        model: Option<ModelId>,
    ) {
        let Some(live) = self.envs.get_mut(env) else {
            return;
        };
        live.waiting_choices.insert(
            terminal,
            ChatChoices {
                model,
                mcp_servers: None,
                effort: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{AgentPreset, VariantSpec};

    fn order_of(harnesses: &[AgentPreset]) -> VariantsOrder {
        VariantsOrder {
            prompt: "Fix it".to_owned(),
            variants: harnesses
                .iter()
                .map(|harness| VariantSpec::new(*harness, None, None))
                .collect(),
            test: None,
        }
    }

    #[test]
    fn a_run_with_a_terminal_harness_is_refused_before_any_worktree_exists() {
        assert!(every_variant_is_a_worker(&order_of(&[
            AgentPreset::ClaudeCode,
            AgentPreset::Codex
        ])));
        assert!(!every_variant_is_a_worker(&order_of(&[
            AgentPreset::ClaudeCode,
            AgentPreset::Shell
        ])));
    }
}
