mod check_in;

use gpui_kit::{AppContext as _, Context};
use krad_agents::ErrorCode;
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AgentPreset, Conversation, Effort, EnvironmentName, GroupId, KraddieOutcome, KraddieRunId,
    KraddieSessionLink, KraddieWorkspace, ModelId, PermissionMode, Prompt, ResumeId, RunRefusal,
    TerminalId, WorkerSpawn, WorkerSurface, WorkingCopy, WorktreeOrigin,
};
use krad_repo::add_worktree;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::BuildHasher;
use std::time::SystemTime;

use super::super::session_templates::ChatChoices;
use super::super::sessions_view::session_terminals;
use super::super::terminal_lifecycle::WorkerChat;
use super::super::EnvironmentShell;
use super::{KraddieWakeOrder, LiveRun, RunOwner};

const WORKTREE_PREFIX: &str = "kraddie";
const BRANCH_SUFFIX_MASK: u64 = 0x00ff_ffff;
const SESSION_FAILED: &str = "The automation's session could not open";
const TASK_SEPARATOR: &str = " · ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum KraddieWakeError {
    Unknown,
    Off,
    NoWorkspace,
    EnvironmentClosed,
    GroupClosed,
    NotAChatAgent,
    Refused(RunRefusal),
}

impl fmt::Display for KraddieWakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("No automation with that name"),
            Self::Off => f.write_str("The automation or its Kraddie is switched off"),
            Self::NoWorkspace => f.write_str(
                "The Kraddie has no repository to work in; edit it in the Kraddies panel",
            ),
            Self::EnvironmentClosed => f.write_str("The Kraddie's environment is not open"),
            Self::GroupClosed => f.write_str("The automation's sidebar group is not open"),
            Self::NotAChatAgent => {
                f.write_str("The automation's agent cannot run as a worker chat")
            }
            Self::Refused(refusal) => refusal.fmt(f),
        }
    }
}

impl KraddieWakeError {
    pub(in crate::environment_shell) const fn code(&self) -> ErrorCode {
        match self {
            Self::Unknown => ErrorCode::NotFound,
            Self::Off => ErrorCode::Forbidden,
            Self::NoWorkspace | Self::NotAChatAgent => ErrorCode::Unsupported,
            Self::EnvironmentClosed | Self::GroupClosed | Self::Refused(_) => {
                ErrorCode::Unavailable
            }
        }
    }
}

struct RunPlan {
    owner: RunOwner,
    run: KraddieRunId,
    workspace: KraddieWorkspace,
    branch: String,
    task: String,
    harness: AgentPreset,
    model: Option<ModelId>,
    effort: Option<Effort>,
    mode: Option<PermissionMode>,
    resume: Option<ResumeId>,
    place: RunPlace,
}

enum RunPlace {
    Worktree,
    Repo,
    Group(GroupId),
}

fn run_branch(automation: &str, run: u64, nonce: u64) -> String {
    format!(
        "{WORKTREE_PREFIX}-{automation}-{run}-{:06x}",
        nonce & BRANCH_SUFFIX_MASK
    )
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn wake_kraddie(
        &mut self,
        order: &KraddieWakeOrder,
        cx: &mut Context<Self>,
    ) -> Result<KraddieRunId, KraddieWakeError> {
        let (kraddie, entry) = self
            .kraddies
            .automation(&order.key)
            .ok_or(KraddieWakeError::Unknown)?;
        let on = kraddie.kraddie.switch.is_on() && entry.automation.switch.is_on();
        if !on && !order.wake.by_user() {
            return Err(KraddieWakeError::Off);
        }
        let workspace = kraddie
            .kraddie
            .workspace
            .clone()
            .ok_or(KraddieWakeError::NoWorkspace)?;
        if !self.envs.contains_key(&workspace.environment) {
            return Err(KraddieWakeError::EnvironmentClosed);
        }
        let automation = &entry.automation;
        let harness = automation.harness.unwrap_or(kraddie.kraddie.harness);
        if harness.worker_surface() != WorkerSurface::Chat {
            return Err(KraddieWakeError::NotAChatAgent);
        }
        let place = match (&automation.session.group, automation.session.working_copy) {
            (Some(root), _) => RunPlace::Group(
                self.group_with_root(&workspace.environment, root)
                    .ok_or(KraddieWakeError::GroupClosed)?,
            ),
            (None, WorkingCopy::ThisRepo) => RunPlace::Repo,
            (None, WorkingCopy::FreshWorktree) => RunPlace::Worktree,
        };
        let resume = (automation.session.conversation == Conversation::ContinueLast)
            .then(|| entry.history.last_resume().cloned())
            .flatten();
        let settings = &kraddie.kraddie.settings;
        let plan_parts = (
            format!(
                "{}{TASK_SEPARATOR}{}",
                kraddie.kraddie.name, automation.name
            ),
            automation
                .model
                .clone()
                .or_else(|| settings.session_model.clone())
                .or_else(|| kraddie.kraddie.model.clone()),
            automation.effort.or(settings.effort),
            automation.permission_mode.or(settings.permissions),
        );
        let mind = self.kraddie_mind(&order.key.kraddie);
        let kraddie_name = kraddie.kraddie.name.clone();
        let started = SystemTime::now();
        if let Err(refusal) = self.kraddies.limiter.begin(&order.key) {
            self.record_kraddie_refusal(order, started, KraddieOutcome::Refused { refusal });
            return Err(KraddieWakeError::Refused(refusal));
        }
        let Some(entry) = self.kraddies.automation_mut(&order.key) else {
            self.kraddies.limiter.end(&order.key);
            return Err(KraddieWakeError::Unknown);
        };
        let run = entry
            .history
            .start(started, order.wake, KraddieOutcome::Starting);
        let routine_brief =
            entry
                .automation
                .brief(&kraddie_name, order.wake, order.prompt.as_deref());
        self.kraddies.dirty.insert(order.key.clone());
        let (task, model, effort, mode) = plan_parts;
        let plan = RunPlan {
            branch: run_branch(
                &order
                    .key
                    .kraddie
                    .numbered(u64::from(order.key.automation.0)),
                run.0,
                RandomState::new().hash_one(started),
            ),
            owner: RunOwner::Automation(order.key.clone()),
            run,
            workspace,
            task,
            harness,
            model,
            effort,
            mode,
            resume,
            place,
        };
        let brief = self.kraddies.entry(&order.key.kraddie).map_or_else(
            || routine_brief.clone(),
            |kraddie| mind.inject(&routine_brief, &kraddie.kraddie),
        );
        tracing::info!(automation = %order.key, run = run.0, wake = order.wake.label(), "automation woke");
        self.count_kraddie_wake(&order.key.kraddie, order.wake, cx);
        self.open_run_place(plan, brief, cx);
        cx.notify();
        Ok(run)
    }

    fn group_with_root(&self, env: &EnvironmentName, root: &std::path::Path) -> Option<GroupId> {
        self.envs
            .get(env)?
            .data
            .groups
            .iter()
            .find(|group| group.root == root)
            .map(|group| group.id)
    }

    fn open_run_place(&mut self, plan: RunPlan, brief: String, cx: &mut Context<Self>) {
        let env = plan.workspace.environment.clone();
        match plan.place {
            RunPlace::Worktree => self.open_kraddie_worktree(plan, brief, cx),
            RunPlace::Group(group) => self.open_kraddie_session(&plan, Some(group), brief, cx),
            RunPlace::Repo => {
                let group = self
                    .group_with_root(&env, &plan.workspace.repo)
                    .or_else(|| self.add_group_to(&env, plan.workspace.repo.clone(), None, cx));
                self.open_kraddie_session(&plan, group, brief, cx);
            }
        }
    }

    fn record_kraddie_refusal(
        &mut self,
        order: &KraddieWakeOrder,
        started: SystemTime,
        outcome: KraddieOutcome,
    ) {
        let Some(entry) = self.kraddies.automation_mut(&order.key) else {
            return;
        };
        let repeated = entry
            .history
            .latest()
            .is_some_and(|run| run.outcome == outcome);
        if repeated {
            return;
        }
        entry.history.start(started, order.wake, outcome);
        self.kraddies.dirty.insert(order.key.clone());
    }

    #[tracing::instrument(skip_all, fields(kraddie = %plan.owner.kraddie(), run = plan.run.0))]
    fn open_kraddie_worktree(&mut self, plan: RunPlan, brief: String, cx: &mut Context<Self>) {
        let link = self.env_link(&plan.workspace.environment);
        let create = cx.background_spawn({
            let repo = plan.workspace.repo.clone();
            let branch = plan.branch.clone();
            async move { add_worktree(&link, &repo, &branch) }
        });
        cx.spawn(async move |this, cx| {
            let created = create.await;
            this.update(cx, |shell, cx| match created {
                Ok(worktree) => {
                    let origin = worktree.branch.map(|branch| WorktreeOrigin {
                        repo_root: plan.workspace.repo.clone(),
                        branch,
                    });
                    let env = plan.workspace.environment.clone();
                    let group = shell.add_group_to(&env, worktree.path, origin, cx);
                    if let Some(group) = group {
                        shell.kraddies.groups.insert((env, group));
                    }
                    shell.open_kraddie_session(&plan, group, brief, cx);
                }
                Err(error) => shell.fail_kraddie_run(&plan, &error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn open_kraddie_session(
        &mut self,
        plan: &RunPlan,
        group: Option<GroupId>,
        brief: String,
        cx: &mut Context<Self>,
    ) {
        let env = plan.workspace.environment.clone();
        let resume = plan.resume.clone().unwrap_or_else(new_session_id);
        let session = if plan.resume.is_some() {
            LiveSession::Resume(resume.clone())
        } else {
            LiveSession::Start(resume.clone())
        };
        let chat = WorkerChat {
            group,
            preset: plan.harness,
            spawn: WorkerSpawn {
                lead: None,
                task: plan.task.clone(),
            },
            session,
            prompts: vec![Prompt::from(brief)],
            mode: plan.mode,
        };
        let opened = self
            .alloc_terminal_id(&env)
            .and_then(|id| self.open_worker_chat(&env, id, chat, cx));
        let Some(terminal) = opened else {
            return self.fail_kraddie_run(plan, SESSION_FAILED, cx);
        };
        self.adopt_kraddie_terminal(&env, terminal, plan, resume);
        match &plan.owner {
            RunOwner::CheckIn(kraddie) => self.note_check_in(&env, terminal, kraddie),
            RunOwner::Automation(key) => self.count_kraddie_session(&key.kraddie, cx),
        }
        cx.notify();
    }

    fn adopt_kraddie_terminal(
        &mut self,
        env: &EnvironmentName,
        terminal: TerminalId,
        plan: &RunPlan,
        resume: ResumeId,
    ) {
        let session = self.envs.get_mut(env).and_then(|live| {
            if plan.model.is_some() || plan.effort.is_some() {
                live.waiting_choices.insert(
                    terminal,
                    ChatChoices {
                        model: plan.model.clone(),
                        mcp_servers: None,
                        effort: plan.effort,
                    },
                );
            }
            live.data
                .sessions
                .iter()
                .find(|session| session_terminals(session).any(|id| id == terminal))
                .map(|session| session.id)
        });
        self.kraddies.runs.insert(
            (env.clone(), terminal),
            LiveRun {
                owner: plan.owner.clone(),
                run: plan.run,
            },
        );
        let history = match &plan.owner {
            RunOwner::Automation(key) => self
                .kraddies
                .automation_mut(key)
                .map(|entry| &mut entry.history),
            RunOwner::CheckIn(kraddie) => self
                .kraddies
                .entry_mut(kraddie)
                .map(|entry| &mut entry.check_ins),
        };
        if let Some(run) = history.and_then(|history| history.run_mut(plan.run)) {
            run.outcome = KraddieOutcome::Working;
            run.resume = Some(resume);
            run.session = session.map(|session| KraddieSessionLink {
                environment: env.clone(),
                session,
                terminal,
            });
        }
        if let RunOwner::Automation(key) = &plan.owner {
            self.kraddies.dirty.insert(key.clone());
        }
    }

    fn fail_kraddie_run(&mut self, plan: &RunPlan, reason: &str, cx: &mut Context<Self>) {
        tracing::warn!(kraddie = %plan.owner.kraddie(), run = plan.run.0, "kraddie run failed");
        let outcome = KraddieOutcome::Failed {
            reason: reason.to_owned(),
        };
        self.kraddies.settle(&plan.owner, plan.run, outcome);
        self.toast(&format!("{}: {reason}", plan.task), cx);
        self.save_histories(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_keep_stable_error_codes() {
        assert_eq!(KraddieWakeError::Unknown.code(), ErrorCode::NotFound);
        assert_eq!(KraddieWakeError::Off.code(), ErrorCode::Forbidden);
        assert_eq!(KraddieWakeError::NoWorkspace.code(), ErrorCode::Unsupported);
        assert_eq!(KraddieWakeError::GroupClosed.code(), ErrorCode::Unavailable);
        assert_eq!(
            KraddieWakeError::Refused(RunRefusal::AtCapacity).code(),
            ErrorCode::Unavailable
        );
    }

    #[test]
    fn two_runs_with_the_same_number_get_different_branches() {
        assert_eq!(
            run_branch("night-owl.2", 7, 0xab_cdef_1234),
            "kraddie-night-owl.2-7-ef1234"
        );
        let started = SystemTime::now();
        let first = run_branch("night-owl.2", 7, RandomState::new().hash_one(started));
        let second = run_branch("night-owl.2", 7, RandomState::new().hash_one(started));
        assert!(first.starts_with("kraddie-night-owl.2-7-"));
        assert_ne!(first, second);
    }
}
