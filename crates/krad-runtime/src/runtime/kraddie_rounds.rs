use krad_domain::{
    quiet_since, run_stamp, CheckInContext, Hush, KraddieId, KraddieWake, LocalMoment,
    PermissionMode, RoundsCheck, RoundsMoment, RunSlot, MAX_KRADDIE_RUNS,
};
use krad_protocol::KraddieState;

use super::kraddie_runs::{RunPlace, RunPlan};
use super::Runtime;
use crate::kraddies::RunOwner;
use crate::workers::runs_as_worker;

const CONTEXT_RUNS: usize = 8;
const CONTEXT_NOTES: usize = 12;
const CONTEXT_INBOX: usize = 12;
const CONTEXT_SEPARATOR: &str = " · ";

impl Runtime {
    pub(super) fn check_in_quiet_kraddies(&mut self, now: LocalMoment) {
        let quiet_hours = self
            .settings
            .current
            .attention
            .quiet_hours
            .contains(now.time_of_day());
        let at_capacity = self.kraddies.limiter.len() >= MAX_KRADDIE_RUNS;
        let due: Vec<KraddieId> = self
            .kraddies
            .states()
            .iter()
            .filter(|state| {
                let id = state.id();
                let running = self.kraddies.limiter.has_kraddie_running(&id)
                    || self
                        .kraddies
                        .check_ins
                        .values()
                        .any(|kraddie| *kraddie == id);
                let moment = RoundsMoment {
                    quiet_since: quiet_since(
                        state.last_started(),
                        self.kraddies.opened_at(),
                        state.last_check_in,
                    ),
                    now: now.at,
                    kraddie: state.kraddie.switch,
                    slot: match (running, at_capacity) {
                        (true, _) => RunSlot::KraddieRunning,
                        (false, true) => RunSlot::AllBusy,
                        (false, false) => RunSlot::Free,
                    },
                    hush: if quiet_hours {
                        Hush::QuietHours
                    } else if self.kraddie_over_budget(state.kraddie.workspace.as_ref()) {
                        Hush::OverBudget
                    } else {
                        Hush::None
                    },
                };
                state.kraddie.settings.rounds.check(&moment) == RoundsCheck::Due
            })
            .map(KraddieState::id)
            .collect();
        for kraddie in due {
            self.check_in_kraddie(&kraddie, now);
        }
    }

    fn check_in_kraddie(&mut self, id: &KraddieId, now: LocalMoment) {
        let Some(state) = self.kraddies.state(id) else {
            return;
        };
        let Some(workspace) = state.kraddie.workspace.clone() else {
            return;
        };
        let harness = state.kraddie.harness;
        if !self.environments.knows(&workspace.environment) || !runs_as_worker(harness) {
            return;
        }
        let settings = &state.kraddie.settings;
        let model = settings
            .session_model
            .clone()
            .or_else(|| state.kraddie.model.clone());
        let (effort, task) = (settings.effort, state.kraddie.name.to_string());
        let brief = state
            .mind
            .check_in_brief(&state.kraddie, &self.check_in_context(state, now));
        let environment = workspace.environment.clone();
        let group = match self.repo_group(&environment, &workspace.repo) {
            Some(group) => Some(group),
            None => self.group_for(&environment, &workspace.repo),
        };
        let Some(group) = group else {
            return;
        };
        let Some(run) = self.kraddies.start_check_in(id, now.at) else {
            return;
        };
        let plan = RunPlan {
            branch: String::new(),
            owner: RunOwner::CheckIn(id.clone()),
            run,
            workspace,
            task,
            harness,
            model,
            effort,
            mode: Some(PermissionMode::Plan),
            resume: None,
            place: RunPlace::Group(group),
            brief,
        };
        tracing::info!(kraddie = %id, run = run.0, "kraddie checks in");
        self.open_run_place(plan);
    }

    fn check_in_context(&self, state: &KraddieState, now: LocalMoment) -> CheckInContext {
        let mut runs: Vec<_> = state
            .automations
            .iter()
            .flat_map(|routine| routine.history.newest_first())
            .chain(state.check_ins.newest_first())
            .collect();
        runs.sort_by_key(|run| std::cmp::Reverse(run.started));
        let recent_work = runs
            .iter()
            .filter(|run| run.wake != KraddieWake::Rounds)
            .take(CONTEXT_RUNS)
            .map(|run| {
                let at = run_stamp(run.started, now.offset);
                [at.as_str(), run.wake.label(), run.outcome.label()].join(CONTEXT_SEPARATOR)
            })
            .collect();
        let sessions_left = runs
            .iter()
            .filter_map(|run| run.session.as_ref())
            .filter_map(|link| {
                let data = self.environments.data(&link.environment)?;
                Some(data.session(link.session)?.name.clone())
            })
            .collect();
        let notes = self
            .kraddies
            .notes
            .iter()
            .take(CONTEXT_NOTES)
            .cloned()
            .collect();
        let inbox = self
            .attention
            .records()
            .take(CONTEXT_INBOX)
            .map(|(kind, title)| [kind, title].join(CONTEXT_SEPARATOR))
            .collect();
        CheckInContext {
            recent_work,
            sessions_left,
            notes,
            inbox,
        }
    }
}
