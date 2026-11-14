use krad_domain::{
    Automation, AutomationId, AutomationKey, EnvironmentName, Kraddie, KraddieHistory, KraddieId,
    KraddieOutcome, KraddieRun, SessionId,
};
use krad_protocol::{KraddieState, KraddiesSnapshot};
use std::rc::Rc;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct AutomationEntry {
    pub(in crate::environment_shell) automation: Automation,
    pub(in crate::environment_shell) history: KraddieHistory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct KraddieEntry {
    pub(in crate::environment_shell) kraddie: Kraddie,
    pub(in crate::environment_shell) automations: Vec<AutomationEntry>,
    pub(in crate::environment_shell) check_ins: KraddieHistory,
}

impl KraddieEntry {
    pub(in crate::environment_shell) fn new(
        kraddie: Kraddie,
        automations: Vec<AutomationEntry>,
    ) -> Self {
        Self {
            kraddie,
            automations,
            check_ins: KraddieHistory::default(),
        }
    }

    pub(in crate::environment_shell) fn automation(
        &self,
        id: AutomationId,
    ) -> Option<&AutomationEntry> {
        self.automations
            .iter()
            .find(|entry| entry.automation.id == id)
    }

    pub(in crate::environment_shell) fn latest_run(&self) -> Option<&KraddieRun> {
        self.automations
            .iter()
            .filter_map(|entry| entry.history.latest())
            .max_by_key(|run| run.started)
    }

    pub(in crate::environment_shell) fn runs_newest_first(&self) -> Vec<&KraddieRun> {
        let mut runs: Vec<&KraddieRun> = self
            .automations
            .iter()
            .flat_map(|entry| entry.history.newest_first())
            .chain(self.check_ins.newest_first())
            .collect();
        runs.sort_by_key(|run| std::cmp::Reverse(run.started));
        runs
    }

    pub(in crate::environment_shell) fn last_started(&self) -> Option<SystemTime> {
        self.automations
            .iter()
            .filter_map(|entry| entry.history.last_started())
            .chain(self.check_ins.last_started())
            .max()
    }

    pub(in crate::environment_shell) fn run_count(&self) -> usize {
        self.automations
            .iter()
            .map(|entry| entry.history.len())
            .sum()
    }

    pub(in crate::environment_shell) fn key(&self, automation: AutomationId) -> AutomationKey {
        AutomationKey::new(self.kraddie.id(), automation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct KraddieBook {
    pub(in crate::environment_shell) entries: Vec<KraddieEntry>,
    pub(in crate::environment_shell) opened_at: SystemTime,
}

impl Default for KraddieBook {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            opened_at: SystemTime::now(),
        }
    }
}

impl KraddieBook {
    pub(in crate::environment_shell) fn entry(&self, id: &KraddieId) -> Option<&KraddieEntry> {
        self.entries.iter().find(|entry| entry.kraddie.id() == *id)
    }

    pub(in crate::environment_shell) fn automation(
        &self,
        key: &AutomationKey,
    ) -> Option<(&KraddieEntry, &AutomationEntry)> {
        let kraddie = self.entry(&key.kraddie)?;
        Some((kraddie, kraddie.automation(key.automation)?))
    }

    pub(in crate::environment_shell) fn run_on(
        &self,
        env: &EnvironmentName,
        session: SessionId,
    ) -> Option<(&KraddieEntry, &KraddieOutcome)> {
        self.entries.iter().find_map(|entry| {
            let run = entry
                .automations
                .iter()
                .flat_map(|routine| routine.history.newest_first())
                .chain(entry.check_ins.newest_first())
                .filter(|run| {
                    run.session
                        .as_ref()
                        .is_some_and(|link| link.environment == *env && link.session == session)
                })
                .min_by_key(|run| std::cmp::Reverse(run.started))?;
            Some((entry, &run.outcome))
        })
    }
}

impl From<&KraddieState> for KraddieEntry {
    fn from(state: &KraddieState) -> Self {
        let automations = state
            .automations
            .iter()
            .map(|state| AutomationEntry {
                automation: state.automation.clone(),
                history: state.history.clone(),
            })
            .collect();
        let mut entry = Self::new(state.kraddie.clone(), automations);
        entry.check_ins = state.check_ins.clone();
        entry
    }
}

#[derive(Default)]
pub(in crate::environment_shell) struct KraddiesMirror {
    book: Rc<KraddieBook>,
}

impl KraddiesMirror {
    pub(in crate::environment_shell) fn current(&self) -> Rc<KraddieBook> {
        Rc::clone(&self.book)
    }

    pub(in crate::environment_shell) fn replace(&mut self, snapshot: &KraddiesSnapshot) {
        let book = Rc::make_mut(&mut self.book);
        if let Some(opened_at) = snapshot.opened_at {
            book.opened_at = opened_at;
        }
        book.entries = snapshot.kraddies.iter().map(KraddieEntry::from).collect();
        sort(&mut book.entries);
    }

    pub(in crate::environment_shell) fn changed(&mut self, state: &KraddieState) {
        let entries = &mut Rc::make_mut(&mut self.book).entries;
        let id = state.id();
        entries.retain(|entry| entry.kraddie.id() != id);
        entries.push(KraddieEntry::from(state));
        sort(entries);
    }

    pub(in crate::environment_shell) fn removed(&mut self, id: &KraddieId) {
        Rc::make_mut(&mut self.book)
            .entries
            .retain(|entry| entry.kraddie.id() != *id);
    }
}

fn sort(entries: &mut [KraddieEntry]) {
    entries.sort_by_key(|entry| entry.kraddie.name.as_str().to_lowercase());
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::AgentPreset;

    fn state(name: &str) -> KraddieState {
        let name = name.parse().unwrap_or_else(|_| unreachable!());
        KraddieState::new(Kraddie::new(name, AgentPreset::ClaudeCode))
    }

    #[test]
    fn the_book_folds_kraddies_sorted_by_name_and_forgets_removed_ones() {
        let mut mirror = KraddiesMirror::default();
        let shared = mirror.current();
        mirror.replace(&KraddiesSnapshot {
            opened_at: None,
            kraddies: vec![state("Zed"), state("amber")],
            digests: Vec::new(),
        });
        let names = |book: &KraddieBook| -> Vec<String> {
            book.entries
                .iter()
                .map(|entry| entry.kraddie.name.as_str().to_owned())
                .collect()
        };
        assert_eq!(names(&mirror.current()), ["amber", "Zed"]);
        assert!(shared.entries.is_empty(), "a read handed out never changes");
        mirror.changed(&state("Milo"));
        assert_eq!(names(&mirror.current()), ["amber", "Milo", "Zed"]);
        let milo = state("Milo").id();
        assert!(mirror.current().entry(&milo).is_some());
        mirror.removed(&milo);
        assert_eq!(names(&mirror.current()), ["amber", "Zed"]);
    }
}
