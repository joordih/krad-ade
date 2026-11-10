mod bus;
mod clock;
mod den_card;
mod editor;
mod events;
mod farewell;
mod layout;
mod motion;
mod panel;
mod profile;
mod rows;
mod run;
mod segmented;
mod shelf;
mod studio;
mod tile;
mod transfer;

use gpui_kit::{AppContext as _, Context, Task};
use krad_domain::{
    AgentState, Automation, AutomationId, AutomationKey, EnvironmentName, GroupId, Kraddie,
    KraddieHistory, KraddieId, KraddieMood, KraddieOutcome, KraddieRun, KraddieRunId,
    KraddieRunLimiter, KraddieWake, SessionId, TerminalId,
};
use krad_store::{KraddieShelf, ShelvedKraddie};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use super::EnvironmentShell;
use crate::kraddie_avatar::KraddieSprite;

pub(super) use clock::next_run;
pub(super) use editor::KraddieEditor;
pub(super) use studio::KraddieStudio;

pub(super) struct AutomationEntry {
    pub(super) automation: Automation,
    pub(super) history: KraddieHistory,
}

pub(super) struct KraddieEntry {
    pub(super) kraddie: Kraddie,
    pub(super) automations: Vec<AutomationEntry>,
    pub(super) check_ins: KraddieHistory,
    sprites: [Rc<KraddieSprite>; 5],
}

impl KraddieEntry {
    pub(super) fn new(kraddie: Kraddie, automations: Vec<AutomationEntry>) -> Self {
        let sprites =
            KraddieMood::ALL.map(|mood| Rc::new(KraddieSprite::new(&kraddie.design, mood)));
        Self {
            kraddie,
            automations,
            check_ins: KraddieHistory::default(),
            sprites,
        }
    }

    pub(super) fn sprite(&self, mood: KraddieMood) -> &Rc<KraddieSprite> {
        &self.sprites[mood.index()]
    }

    pub(super) fn automation(&self, id: AutomationId) -> Option<&AutomationEntry> {
        self.automations
            .iter()
            .find(|entry| entry.automation.id == id)
    }

    fn automation_mut(&mut self, id: AutomationId) -> Option<&mut AutomationEntry> {
        self.automations
            .iter_mut()
            .find(|entry| entry.automation.id == id)
    }

    pub(super) fn default_automation(&self) -> Option<&AutomationEntry> {
        self.automations
            .iter()
            .find(|entry| entry.automation.is_manual())
            .or_else(|| self.automations.first())
    }

    pub(super) fn latest_run(&self) -> Option<&KraddieRun> {
        self.automations
            .iter()
            .filter_map(|entry| entry.history.latest())
            .max_by_key(|run| run.started)
    }

    pub(super) fn runs_newest_first(&self) -> Vec<&KraddieRun> {
        let mut runs: Vec<&KraddieRun> = self
            .automations
            .iter()
            .flat_map(|entry| entry.history.newest_first())
            .chain(self.check_ins.newest_first())
            .collect();
        runs.sort_by_key(|run| std::cmp::Reverse(run.started));
        runs
    }

    pub(super) fn last_started(&self) -> Option<SystemTime> {
        self.automations
            .iter()
            .filter_map(|entry| entry.history.last_started())
            .chain(self.check_ins.last_started())
            .max()
    }

    pub(super) fn run_count(&self) -> usize {
        self.automations
            .iter()
            .map(|entry| entry.history.len())
            .sum()
    }

    pub(super) fn key(&self, automation: AutomationId) -> AutomationKey {
        AutomationKey::new(self.kraddie.id(), automation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RunOwner {
    Automation(AutomationKey),
    CheckIn(KraddieId),
}

impl RunOwner {
    pub(super) const fn kraddie(&self) -> &KraddieId {
        match self {
            Self::Automation(key) => &key.kraddie,
            Self::CheckIn(kraddie) => kraddie,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LiveRun {
    owner: RunOwner,
    run: KraddieRunId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct KraddieWakeOrder {
    pub(super) key: AutomationKey,
    pub(super) wake: KraddieWake,
    pub(super) prompt: Option<String>,
}

pub(super) struct KraddiesUi {
    shelf: Option<KraddieShelf>,
    pub(super) entries: Vec<KraddieEntry>,
    limiter: KraddieRunLimiter,
    runs: HashMap<(EnvironmentName, TerminalId), LiveRun>,
    groups: HashSet<(EnvironmentName, GroupId)>,
    wakes: Vec<KraddieWakeOrder>,
    dirty: HashSet<AutomationKey>,
    opened_at: SystemTime,
    started: Instant,
    pub(super) editor: Option<KraddieEditor>,
    pub(super) studio: Option<KraddieStudio>,
    pub(super) profile: profile::ProfileUi,
    pub(super) memory: super::kraddie_memory::KraddieMemoryUi,
    deleting: Option<KraddieId>,
    animated: Cell<bool>,
    ticker: Option<Task<()>>,
    placeholder: std::cell::OnceCell<Rc<KraddieSprite>>,
}

impl Default for KraddiesUi {
    fn default() -> Self {
        Self {
            shelf: KraddieShelf::in_config(),
            entries: Vec::new(),
            limiter: KraddieRunLimiter::default(),
            runs: HashMap::new(),
            groups: HashSet::new(),
            wakes: Vec::new(),
            dirty: HashSet::new(),
            opened_at: SystemTime::now(),
            started: Instant::now(),
            editor: None,
            studio: None,
            profile: profile::ProfileUi::default(),
            memory: super::kraddie_memory::KraddieMemoryUi::default(),
            deleting: None,
            animated: Cell::new(false),
            ticker: None,
            placeholder: std::cell::OnceCell::new(),
        }
    }
}

impl KraddiesUi {
    pub(super) fn entry(&self, id: &KraddieId) -> Option<&KraddieEntry> {
        self.entries.iter().find(|entry| entry.kraddie.id() == *id)
    }

    fn entry_mut(&mut self, id: &KraddieId) -> Option<&mut KraddieEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.kraddie.id() == *id)
    }

    pub(super) fn automation(
        &self,
        key: &AutomationKey,
    ) -> Option<(&KraddieEntry, &AutomationEntry)> {
        let kraddie = self.entry(&key.kraddie)?;
        Some((kraddie, kraddie.automation(key.automation)?))
    }

    fn automation_mut(&mut self, key: &AutomationKey) -> Option<&mut AutomationEntry> {
        self.entry_mut(&key.kraddie)?.automation_mut(key.automation)
    }

    pub(super) const fn opened_at(&self) -> SystemTime {
        self.opened_at
    }

    pub(super) fn frame(&self) -> u64 {
        krad_domain::animation_frame(self.started.elapsed())
    }

    pub(super) fn placeholder(&self) -> Rc<KraddieSprite> {
        Rc::clone(self.placeholder.get_or_init(panel::placeholder_sprite))
    }

    pub(super) fn note_drawn(&self, sprite: &KraddieSprite) {
        if sprite.moves() {
            self.animated.set(true);
        }
    }

    fn owner_history(&self, owner: &RunOwner) -> Option<&KraddieHistory> {
        match owner {
            RunOwner::Automation(key) => self.automation(key).map(|(_, entry)| &entry.history),
            RunOwner::CheckIn(kraddie) => self.entry(kraddie).map(|entry| &entry.check_ins),
        }
    }

    fn settle(&mut self, owner: &RunOwner, run: KraddieRunId, outcome: KraddieOutcome) {
        let key = match owner {
            RunOwner::Automation(key) => key,
            RunOwner::CheckIn(kraddie) => {
                if let Some(entry) = self.entry_mut(kraddie) {
                    entry.check_ins.settle(run, outcome);
                }
                return;
            }
        };
        let ends = !outcome.is_live();
        if let Some(entry) = self.automation_mut(key) {
            if entry.history.settle(run, outcome) {
                self.dirty.insert(key.clone());
            }
        }
        if ends {
            self.limiter.end(key);
        }
    }

    fn run_on(
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

impl EnvironmentShell {
    pub(super) fn load_kraddies(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.kraddies.shelf.clone() else {
            return;
        };
        let read = cx.background_spawn(async move {
            for error in store.settle_folder() {
                tracing::warn!(%error, "old Kraddie file not moved; it loads where it is");
            }
            store.read_all()
        });
        cx.spawn(async move |this, cx| {
            let kraddies = read.await;
            this.update(cx, |shell, cx| shell.adopt_kraddies(kraddies, cx))
                .unwrap_or_default();
        })
        .detach();
        self.start_kraddie_clock(cx);
        self.start_memory_clock(cx);
    }

    fn adopt_kraddies(&mut self, kraddies: Vec<ShelvedKraddie>, cx: &mut Context<Self>) {
        for shelved in kraddies {
            let id = shelved.kraddie.id();
            if let Some(problem) = &shelved.migration_problem {
                tracing::warn!(kraddie = %id, %problem, "kraddie automations not migrated");
            }
            let automations = shelved
                .automations
                .into_iter()
                .map(|shelved| {
                    let mut history = shelved.history;
                    if history.interrupt_live() {
                        self.kraddies
                            .dirty
                            .insert(AutomationKey::new(id.clone(), shelved.automation.id));
                    }
                    AutomationEntry {
                        automation: shelved.automation,
                        history,
                    }
                })
                .collect();
            self.kraddies
                .entries
                .push(KraddieEntry::new(shelved.kraddie, automations));
        }
        tracing::info!(kraddies = self.kraddies.entries.len(), "kraddies loaded");
        self.queue_missed_runs();
        self.load_kraddie_profiles(cx);
        self.flush_kraddies(cx);
        cx.notify();
    }

    pub(super) fn flush_kraddies(&mut self, cx: &mut Context<Self>) {
        for order in std::mem::take(&mut self.kraddies.wakes) {
            if let Err(refusal) = self.wake_kraddie(&order, cx) {
                tracing::debug!(automation = %order.key, %refusal, "automation did not run");
            }
        }
        self.settle_ended_kraddie_runs(cx);
        self.save_histories(cx);
    }

    pub(super) fn session_kraddie_look(
        &self,
        env: &EnvironmentName,
        session: SessionId,
        agent: Option<AgentState>,
    ) -> Option<Rc<KraddieSprite>> {
        let Some((entry, outcome)) = self.kraddies.run_on(env, session) else {
            return self.kraddie_chat_look(env, session);
        };
        let sprite = entry.sprite(KraddieMood::of_run(agent, Some(outcome)));
        self.kraddies.note_drawn(sprite);
        Some(Rc::clone(sprite))
    }

    pub(super) fn kraddies_frame(&self) -> u64 {
        self.kraddies.frame()
    }

    pub(super) fn kraddie_mood(&self, entry: &KraddieEntry) -> KraddieMood {
        let Some(run) = entry.latest_run() else {
            return KraddieMood::Idle;
        };
        if !run.outcome.is_live() {
            return KraddieMood::of_outcome(&run.outcome);
        }
        let agent = run.session.as_ref().and_then(|link| {
            let live = self.envs.get(&link.environment)?;
            live.data
                .nodes
                .iter()
                .find(|node| node.id == link.terminal)
                .map(|node| node.state)
        });
        KraddieMood::of_run(agent, Some(&run.outcome))
    }
}
