mod bus;
mod digests;
mod edits;
mod memory;
mod runs;
mod schedule;
mod writer;

use krad_domain::{
    AutomationKey, DigestBook, EnvironmentName, GroupId, KraddieDay, KraddieId, KraddieRunId,
    KraddieRunLimiter, LocalMoment, MemoryOwner,
};
use krad_protocol::{AgentRef, EventKind, KraddieState, KraddiesSnapshot, OwnedDigests};
use krad_store::{KraddieShelf, OrdinaryDigests};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub(crate) use bus::{automation_info, kraddie_infos, KraddieWakeError};
pub(crate) use digests::{reply_sink, DigestReply, DigestWatch, DigestWriter, Watched};
pub(crate) use memory::{chat_owner, memory_answer};
pub(crate) use schedule::WakeOrder;
use writer::{ShelfWrite, ShelfWriter};

use crate::input::Input;

const SCHEDULE_CHECK: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> LocalMoment + Send + Sync>);

impl Clock {
    #[must_use]
    pub fn new(read: impl Fn() -> LocalMoment + Send + Sync + 'static) -> Self {
        Self(Arc::new(read))
    }

    #[must_use]
    pub fn now(&self) -> LocalMoment {
        (self.0)()
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new(|| LocalMoment::utc(SystemTime::now()))
    }
}

impl fmt::Debug for Clock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Clock").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default)]
pub struct KraddiesSetup {
    pub shelf: Option<KraddieShelf>,
    pub ordinary: Option<OrdinaryDigests>,
    pub clock: Clock,
}

impl KraddiesSetup {
    #[must_use]
    pub fn in_config_folder(clock: Clock) -> Self {
        Self {
            shelf: KraddieShelf::in_config(),
            ordinary: OrdinaryDigests::in_config()
                .inspect_err(|error| tracing::debug!(%error, "no folder for chat digests"))
                .ok(),
            clock,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RunOwner {
    Automation(AutomationKey),
    CheckIn(KraddieId),
}

impl RunOwner {
    pub(crate) const fn kraddie(&self) -> &KraddieId {
        match self {
            Self::Automation(key) => &key.kraddie,
            Self::CheckIn(kraddie) => kraddie,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveRun {
    pub(crate) owner: RunOwner,
    pub(crate) run: KraddieRunId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Dirty {
    Kraddie(KraddieId),
    Automations(KraddieId),
    History(AutomationKey),
    Mind(KraddieId),
    Activity(KraddieId),
    Digests(MemoryOwner),
}

#[derive(Debug)]
pub(crate) struct Kraddies {
    states: Vec<KraddieState>,
    ordinary: DigestBook,
    books: HashMap<KraddieId, DigestBook>,
    writer: Option<ShelfWriter>,
    pub(crate) inbox: SyncSender<Input>,
    clock: Clock,
    opened_at: SystemTime,
    next_check: SystemTime,
    pub(crate) limiter: KraddieRunLimiter,
    pub(crate) runs: HashMap<AgentRef, LiveRun>,
    pub(crate) groups: HashSet<(EnvironmentName, GroupId)>,
    pub(crate) check_ins: HashMap<AgentRef, KraddieId>,
    pub(crate) notes: Vec<String>,
    pub(crate) digests: DigestWatch,
    changed: BTreeSet<KraddieId>,
    removed: BTreeSet<KraddieId>,
    changed_books: BTreeSet<MemoryOwner>,
    dirty: BTreeSet<Dirty>,
}

impl Kraddies {
    pub(crate) fn start(setup: KraddiesSetup, inbox: SyncSender<Input>) -> Self {
        let opened_at = setup.clock.now().at;
        let mut kraddies = Self {
            states: Vec::new(),
            ordinary: DigestBook::default(),
            books: HashMap::new(),
            writer: None,
            inbox,
            clock: setup.clock,
            opened_at,
            next_check: opened_at,
            limiter: KraddieRunLimiter::default(),
            runs: HashMap::new(),
            groups: HashSet::new(),
            check_ins: HashMap::new(),
            notes: Vec::new(),
            digests: DigestWatch::default(),
            changed: BTreeSet::new(),
            removed: BTreeSet::new(),
            changed_books: BTreeSet::new(),
            dirty: BTreeSet::new(),
        };
        if let Some(shelf) = &setup.shelf {
            kraddies.read_shelf(shelf);
        }
        if let Some(ordinary) = &setup.ordinary {
            kraddies.ordinary = ordinary.read().unwrap_or_else(|error| {
                tracing::warn!(%error, "chat digests unreadable; set aside");
                DigestBook::default()
            });
        }
        kraddies.writer = ShelfWriter::spawn(setup.shelf, setup.ordinary);
        tracing::info!(kraddies = kraddies.states.len(), "kraddies loaded");
        kraddies
    }

    fn read_shelf(&mut self, files: &KraddieShelf) {
        for error in files.settle_folder() {
            tracing::warn!(%error, "old Kraddie file not moved; it loads where it is");
        }
        let today = self.clock.now().date();
        for shelved in files.read_all() {
            let id = shelved.kraddie.id();
            if let Some(problem) = &shelved.migration_problem {
                tracing::warn!(kraddie = %id, %problem, "kraddie automations not migrated");
            }
            let mut state = KraddieState::new(shelved.kraddie);
            for shelved in shelved.automations {
                let mut history = shelved.history;
                if history.interrupt_live() {
                    let key = AutomationKey::new(id.clone(), shelved.automation.id);
                    self.dirty.insert(Dirty::History(key));
                }
                state.automations.push(krad_protocol::AutomationState {
                    automation: shelved.automation,
                    history,
                });
            }
            let profile = files.read_profile(&id);
            state.mind = profile.mind;
            state.activity = profile.activity;
            state.activity.prune(today);
            match files.read_digests(&id) {
                Ok(book) => {
                    self.books.insert(id, book);
                }
                Err(error) => tracing::warn!(kraddie = %id, %error, "chat digests unreadable"),
            }
            self.states.push(state);
        }
        self.sort();
    }

    fn sort(&mut self) {
        self.states
            .sort_by_key(|state| state.kraddie.name.as_str().to_lowercase());
    }

    pub(crate) fn now(&self) -> LocalMoment {
        self.clock.now()
    }

    pub(crate) const fn opened_at(&self) -> SystemTime {
        self.opened_at
    }

    pub(crate) fn states(&self) -> &[KraddieState] {
        &self.states
    }

    pub(crate) fn state(&self, id: &KraddieId) -> Option<&KraddieState> {
        self.states.iter().find(|state| state.id() == *id)
    }

    pub(crate) fn state_mut(&mut self, id: &KraddieId) -> Option<&mut KraddieState> {
        self.changed.insert(id.clone());
        self.states.iter_mut().find(|state| state.id() == *id)
    }

    pub(crate) fn book(&self, owner: &MemoryOwner) -> Option<&DigestBook> {
        match owner {
            MemoryOwner::Ordinary => Some(&self.ordinary),
            MemoryOwner::Kraddie(id) => self.books.get(id),
        }
    }

    pub(crate) fn book_mut(&mut self, owner: &MemoryOwner) -> &mut DigestBook {
        self.changed_books.insert(owner.clone());
        self.dirty.insert(Dirty::Digests(owner.clone()));
        match owner {
            MemoryOwner::Ordinary => &mut self.ordinary,
            MemoryOwner::Kraddie(id) => self.books.entry(id.clone()).or_default(),
        }
    }

    pub(crate) fn history_changed(&mut self, key: &AutomationKey) {
        self.changed.insert(key.kraddie.clone());
        self.dirty.insert(Dirty::History(key.clone()));
    }

    pub(crate) fn count_day(&mut self, id: &KraddieId, delta: &KraddieDay) {
        let today = self.clock.now().date();
        let Some(state) = self.state_mut(id) else {
            return;
        };
        state.activity.record(today, delta);
        self.dirty.insert(Dirty::Activity(id.clone()));
    }

    pub(crate) fn schedule_due(&mut self, now: SystemTime) -> bool {
        if now < self.next_check {
            return false;
        }
        self.next_check = now + SCHEDULE_CHECK;
        true
    }

    pub(crate) fn snapshot(&self) -> KraddiesSnapshot {
        let digests = std::iter::once(MemoryOwner::Ordinary)
            .chain(self.books.keys().cloned().map(MemoryOwner::Kraddie))
            .filter_map(|owner| {
                let book = self.book(&owner)?.clone();
                Some(OwnedDigests { owner, book })
            })
            .collect();
        KraddiesSnapshot {
            opened_at: Some(self.opened_at),
            kraddies: self.states.clone(),
            digests,
        }
    }

    pub(crate) fn flush(&mut self, outbox: &mut Vec<EventKind>) {
        for kraddie in std::mem::take(&mut self.removed) {
            if self.state(&kraddie).is_none() {
                outbox.push(EventKind::KraddieRemoved { kraddie });
            }
        }
        for id in std::mem::take(&mut self.changed) {
            if let Some(state) = self.state(&id) {
                outbox.push(EventKind::KraddieChanged {
                    kraddie: Box::new(state.clone()),
                });
            }
        }
        for owner in std::mem::take(&mut self.changed_books) {
            if let Some(book) = self.book(&owner) {
                outbox.push(EventKind::DigestsChanged(OwnedDigests {
                    owner,
                    book: book.clone(),
                }));
            }
        }
        for dirty in std::mem::take(&mut self.dirty) {
            if let Some(write) = self.write_of(dirty) {
                self.write(write);
            }
        }
    }

    fn write_of(&self, dirty: Dirty) -> Option<ShelfWrite> {
        Some(match dirty {
            Dirty::Kraddie(id) => ShelfWrite::Kraddie(Box::new(self.state(&id)?.kraddie.clone())),
            Dirty::Automations(id) => {
                let automations = self
                    .state(&id)?
                    .automations
                    .iter()
                    .map(|state| state.automation.clone())
                    .collect();
                ShelfWrite::Automations(id, automations)
            }
            Dirty::History(key) => {
                let history = self
                    .state(&key.kraddie)?
                    .automation(key.automation)?
                    .history
                    .clone();
                ShelfWrite::History(key, history)
            }
            Dirty::Mind(id) => ShelfWrite::Mind(id.clone(), self.state(&id)?.mind.clone()),
            Dirty::Activity(id) => {
                ShelfWrite::Activity(id.clone(), self.state(&id)?.activity.clone())
            }
            Dirty::Digests(owner) => {
                let book = self.book(&owner)?.clone();
                ShelfWrite::Digests(owner, book)
            }
        })
    }

    fn write(&self, write: ShelfWrite) {
        if let Some(writer) = &self.writer {
            writer.send(write);
        }
    }

    pub(crate) fn settle(&self, settled: impl FnOnce() + Send + 'static) {
        match &self.writer {
            Some(writer) => writer.send(ShelfWrite::Then(Box::new(settled))),
            None => settled(),
        }
    }
}
