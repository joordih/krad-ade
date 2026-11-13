use krad_domain::{
    AutomationKey, KraddieId, KraddieOutcome, KraddieRunId, KraddieSessionLink, KraddieWake,
    ResumeId, RunRefusal,
};
use std::time::SystemTime;

use super::{Kraddies, RunOwner, WakeOrder};

impl Kraddies {
    pub(crate) fn record_refusal(
        &mut self,
        order: &WakeOrder,
        started: SystemTime,
        refusal: RunRefusal,
    ) {
        let outcome = KraddieOutcome::Refused { refusal };
        let Some(routine) = self
            .state_mut(&order.key.kraddie)
            .and_then(|state| state.automation_mut(order.key.automation))
        else {
            return;
        };
        if routine
            .history
            .latest()
            .is_some_and(|run| run.outcome == outcome)
        {
            return;
        }
        routine.history.start(started, order.wake, outcome);
        self.history_changed(&order.key);
    }

    pub(crate) fn start_run(
        &mut self,
        key: &AutomationKey,
        wake: KraddieWake,
        started: SystemTime,
    ) -> Option<KraddieRunId> {
        let routine = self
            .state_mut(&key.kraddie)?
            .automation_mut(key.automation)?;
        let run = routine
            .history
            .start(started, wake, KraddieOutcome::Starting);
        self.history_changed(key);
        Some(run)
    }

    pub(crate) fn start_check_in(
        &mut self,
        id: &KraddieId,
        started: SystemTime,
    ) -> Option<KraddieRunId> {
        let state = self.state_mut(id)?;
        state.last_check_in = Some(started);
        Some(
            state
                .check_ins
                .start(started, KraddieWake::Rounds, KraddieOutcome::Starting),
        )
    }

    pub(crate) fn adopt_run(
        &mut self,
        owner: &RunOwner,
        run: KraddieRunId,
        resume: ResumeId,
        link: KraddieSessionLink,
    ) {
        let history = match owner {
            RunOwner::Automation(key) => {
                self.history_changed(key);
                self.state_mut(&key.kraddie)
                    .and_then(|state| state.automation_mut(key.automation))
                    .map(|routine| &mut routine.history)
            }
            RunOwner::CheckIn(kraddie) => self.state_mut(kraddie).map(|state| &mut state.check_ins),
        };
        if let Some(run) = history.and_then(|history| history.run_mut(run)) {
            run.outcome = KraddieOutcome::Working;
            run.resume = Some(resume);
            run.session = Some(link);
        }
    }

    pub(crate) fn settle_run(
        &mut self,
        owner: &RunOwner,
        run: KraddieRunId,
        outcome: KraddieOutcome,
    ) {
        let key = match owner {
            RunOwner::Automation(key) => key,
            RunOwner::CheckIn(kraddie) => {
                if let Some(state) = self.state_mut(kraddie) {
                    state.check_ins.settle(run, outcome);
                }
                return;
            }
        };
        let ends = !outcome.is_live();
        let settled = self
            .state_mut(&key.kraddie)
            .and_then(|state| state.automation_mut(key.automation))
            .is_some_and(|routine| routine.history.settle(run, outcome));
        if settled {
            self.history_changed(key);
        }
        if ends {
            self.limiter.end(key);
        }
    }

    pub(crate) fn forget_check_in(&mut self, kraddie: &KraddieId, run: KraddieRunId) {
        if let Some(state) = self.state_mut(kraddie) {
            state.check_ins.forget(run);
        }
    }

    pub(crate) fn outcome_of(
        &self,
        owner: &RunOwner,
        run: KraddieRunId,
    ) -> Option<&KraddieOutcome> {
        let state = self.state(owner.kraddie())?;
        let history = match owner {
            RunOwner::Automation(key) => &state.automation(key.automation)?.history,
            RunOwner::CheckIn(_) => &state.check_ins,
        };
        history
            .newest_first()
            .find(|past| past.id == run)
            .map(|past| &past.outcome)
    }
}
