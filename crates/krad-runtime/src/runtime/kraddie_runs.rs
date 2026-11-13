use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AgentPreset, Conversation, Effort, EnvironmentName, GroupEdit, GroupId, KraddieDay,
    KraddieOutcome, KraddieRunId, KraddieSessionLink, KraddieWake, KraddieWorkspace, ModelId,
    NodeEdit, NodePlace, PermissionMode, Prompt, ResumeId, TerminalId, WorkerSpawn, WorkingCopy,
};
use krad_protocol::EventKind;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::path::Path;

use super::working::agent;
use super::Runtime;
use crate::chats::{ChatBrief, ChatRole, ChatStart};
use crate::kraddies::{KraddieWakeError, LiveRun, RunOwner, WakeOrder};
use crate::workers::{runs_as_worker, worker_node};

const WORKTREE_PREFIX: &str = "kraddie";
const BRANCH_SUFFIX_MASK: u64 = 0x00ff_ffff;
const SESSION_FAILED: &str = "The automation's session could not open";
const NO_GROUP: &str = "The automation's repository could not open as a group";
const TASK_SEPARATOR: &str = " · ";

#[derive(Debug)]
pub(crate) struct RunPlan {
    pub(super) owner: RunOwner,
    pub(super) run: KraddieRunId,
    pub(super) workspace: KraddieWorkspace,
    pub(super) branch: String,
    pub(super) task: String,
    pub(super) harness: AgentPreset,
    pub(super) model: Option<ModelId>,
    pub(super) effort: Option<Effort>,
    pub(super) mode: Option<PermissionMode>,
    pub(super) resume: Option<ResumeId>,
    pub(super) place: RunPlace,
    pub(super) brief: String,
}

#[derive(Debug)]
pub(super) enum RunPlace {
    Worktree,
    Repo,
    Group(GroupId),
}

fn run_branch(automation: &str, run: u64, uniqueness: u64) -> String {
    format!(
        "{WORKTREE_PREFIX}-{automation}-{run}-{:06x}",
        uniqueness & BRANCH_SUFFIX_MASK
    )
}

impl Runtime {
    pub(super) fn wake_kraddies(&mut self, orders: Vec<WakeOrder>) {
        for order in orders {
            if let Err(refusal) = self.wake_kraddie(&order) {
                tracing::debug!(automation = %order.key, %refusal, "automation did not run");
            }
        }
    }

    pub(super) fn wake_kraddie(
        &mut self,
        order: &WakeOrder,
    ) -> Result<KraddieRunId, KraddieWakeError> {
        let state = self
            .kraddies
            .state(&order.key.kraddie)
            .ok_or(KraddieWakeError::Unknown)?;
        let routine = state
            .automation(order.key.automation)
            .ok_or(KraddieWakeError::Unknown)?;
        let on = state.kraddie.switch.is_on() && routine.automation.switch.is_on();
        if !on && !order.wake.by_user() {
            return Err(KraddieWakeError::Off);
        }
        let workspace = state
            .kraddie
            .workspace
            .clone()
            .ok_or(KraddieWakeError::NoWorkspace)?;
        if !self.environments.knows(&workspace.environment) {
            return Err(KraddieWakeError::EnvironmentClosed);
        }
        let automation = &routine.automation;
        let harness = automation.harness.unwrap_or(state.kraddie.harness);
        if !runs_as_worker(harness) {
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
            .then(|| routine.history.last_resume().cloned())
            .flatten();
        let settings = &state.kraddie.settings;
        let task = format!("{}{TASK_SEPARATOR}{}", state.kraddie.name, automation.name);
        let model = automation
            .model
            .clone()
            .or_else(|| settings.session_model.clone())
            .or_else(|| state.kraddie.model.clone());
        let (effort, mode) = (
            automation.effort.or(settings.effort),
            automation.permission_mode.or(settings.permissions),
        );
        let routine_brief =
            automation.brief(&state.kraddie.name, order.wake, order.prompt.as_deref());
        let brief = state.mind.inject(&routine_brief, &state.kraddie);
        let started = self.kraddies.now().at;
        if let Err(refusal) = self.kraddies.limiter.begin(&order.key) {
            self.kraddies.record_refusal(order, started, refusal);
            return Err(KraddieWakeError::Refused(refusal));
        }
        let Some(run) = self.kraddies.start_run(&order.key, order.wake, started) else {
            self.kraddies.limiter.end(&order.key);
            return Err(KraddieWakeError::Unknown);
        };
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
            brief,
        };
        tracing::info!(automation = %order.key, run = run.0, wake = order.wake.label(), "automation woke");
        if order.wake != KraddieWake::Rounds {
            self.kraddies
                .count_day(&order.key.kraddie, &KraddieDay::of_wake(order.wake));
        }
        self.open_run_place(plan);
        Ok(run)
    }

    pub(super) fn group_with_root(
        &self,
        environment: &EnvironmentName,
        root: &Path,
    ) -> Option<GroupId> {
        self.environments
            .data(environment)?
            .groups
            .iter()
            .find(|group| group.root == root)
            .map(|group| group.id)
    }

    pub(super) fn repo_group(&self, environment: &EnvironmentName, repo: &Path) -> Option<GroupId> {
        self.environments
            .data(environment)?
            .groups
            .iter()
            .find(|group| group.root == repo && group.worktree.is_none())
            .map(|group| group.id)
    }

    pub(super) fn group_for(
        &mut self,
        environment: &EnvironmentName,
        root: &Path,
    ) -> Option<GroupId> {
        if let Some(open) = self.group_with_root(environment, root) {
            return Some(open);
        }
        let group = self.environments.data(environment)?.next_group_id();
        let added = GroupEdit::Add {
            group,
            root: root.to_path_buf(),
            worktree: None,
        };
        self.environments
            .edit(environment.clone(), &added.into(), None, &mut self.outbox);
        self.git.refresh(environment);
        self.group_with_root(environment, root)
    }

    pub(super) fn open_run_place(&mut self, plan: RunPlan) {
        let environment = plan.workspace.environment.clone();
        match plan.place {
            RunPlace::Worktree => self.open_kraddie_worktree(plan),
            RunPlace::Group(group) => self.open_kraddie_session(&plan, group),
            RunPlace::Repo => match self.group_for(&environment, &plan.workspace.repo.clone()) {
                Some(group) => self.open_kraddie_session(&plan, group),
                None => self.fail_kraddie_run(&plan, NO_GROUP),
            },
        }
    }

    pub(super) fn open_kraddie_session(&mut self, plan: &RunPlan, group: GroupId) {
        let environment = plan.workspace.environment.clone();
        let Some(data) = self.environments.data(&environment) else {
            return self.fail_kraddie_run(plan, SESSION_FAILED);
        };
        let terminal = TerminalId(data.next_id);
        let session = data.next_session_id();
        let label = self.settings.current.acp.harness_name(plan.harness);
        let node = worker_node(terminal, &label, plan.harness, group);
        let opened = NodeEdit::Open {
            node: Box::new(node),
            place: NodePlace::Worker {
                session,
                spawn: WorkerSpawn {
                    lead: None,
                    task: plan.task.clone(),
                },
                cloud: None,
            },
        };
        self.environments
            .edit(environment.clone(), &opened.into(), None, &mut self.outbox);
        let placed = self
            .environments
            .data(&environment)
            .and_then(|data| data.session_of(terminal));
        let Some(session) = placed else {
            return self.fail_kraddie_run(plan, SESSION_FAILED);
        };
        let resume = plan.resume.clone().unwrap_or_else(new_session_id);
        let live = if plan.resume.is_some() {
            LiveSession::Resume(resume.clone())
        } else {
            LiveSession::Start(resume.clone())
        };
        let start = ChatStart {
            harness: plan.harness,
            brief: ChatBrief {
                model: plan.model.clone(),
                effort: plan.effort,
                permission_mode: plan.mode,
                mcp_servers: None,
                role: ChatRole::Agent,
            },
            prepared: None,
            session: live,
            messages: Vec::new(),
            queued: vec![Prompt::from(plan.brief.clone())],
            notices: Vec::new(),
        };
        let chat = agent(&environment, terminal);
        self.open_chat(&chat, start);
        let link = KraddieSessionLink {
            environment,
            session,
            terminal,
        };
        self.kraddies.adopt_run(&plan.owner, plan.run, resume, link);
        self.kraddies.runs.insert(
            chat.clone(),
            LiveRun {
                owner: plan.owner.clone(),
                run: plan.run,
            },
        );
        match &plan.owner {
            RunOwner::CheckIn(kraddie) => {
                self.kraddies.check_ins.insert(chat, kraddie.clone());
            }
            RunOwner::Automation(key) => {
                self.kraddies
                    .count_day(&key.kraddie, &KraddieDay::of_session());
            }
        }
    }

    pub(super) fn fail_kraddie_run(&mut self, plan: &RunPlan, reason: &str) {
        tracing::warn!(kraddie = %plan.owner.kraddie(), run = plan.run.0, "kraddie run failed");
        let outcome = KraddieOutcome::Failed {
            reason: reason.to_owned(),
        };
        self.kraddies.settle_run(&plan.owner, plan.run, outcome);
        self.outbox.push(EventKind::KraddieRunFailed {
            task: plan.task.clone(),
            reason: reason.to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_runs_with_the_same_number_get_different_branches() {
        assert_eq!(
            run_branch("night-owl.2", 7, 0xab_cdef_1234),
            "kraddie-night-owl.2-7-ef1234"
        );
        let started = std::time::SystemTime::now();
        let first = run_branch("night-owl.2", 7, RandomState::new().hash_one(started));
        let second = run_branch("night-owl.2", 7, RandomState::new().hash_one(started));
        assert!(first.starts_with("kraddie-night-owl.2-7-"));
        assert_ne!(first, second);
    }
}
