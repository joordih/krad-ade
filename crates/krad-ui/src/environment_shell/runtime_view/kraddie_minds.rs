use krad_domain::{DigestBook, KraddieActivity, KraddieId, KraddieMind, MemoryOwner};
use krad_protocol::{KraddieState, OwnedDigests};
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct KraddieMinds {
    minds: HashMap<KraddieId, KraddieMind>,
    activity: HashMap<KraddieId, KraddieActivity>,
    last_check_in: HashMap<KraddieId, SystemTime>,
    pub(in crate::environment_shell) revision: u64,
}

impl KraddieMinds {
    pub(in crate::environment_shell) fn mind(&self, id: &KraddieId) -> KraddieMind {
        self.minds.get(id).cloned().unwrap_or_default()
    }

    pub(in crate::environment_shell) fn mind_mut(&mut self, id: &KraddieId) -> &mut KraddieMind {
        self.minds.entry(id.clone()).or_default()
    }

    pub(in crate::environment_shell) fn activity(
        &self,
        id: &KraddieId,
    ) -> Option<&KraddieActivity> {
        self.activity.get(id)
    }

    pub(in crate::environment_shell) fn last_check_in(&self, id: &KraddieId) -> Option<SystemTime> {
        self.last_check_in.get(id).copied()
    }

    pub(in crate::environment_shell) fn adopt(&mut self, state: &KraddieState) {
        let id = state.id();
        self.minds.insert(id.clone(), state.mind.clone());
        if self.activity.get(&id) != Some(&state.activity) {
            self.activity.insert(id.clone(), state.activity.clone());
            self.revision = self.revision.wrapping_add(1);
        }
        match state.last_check_in {
            Some(at) => self.last_check_in.insert(id, at),
            None => self.last_check_in.remove(&id),
        };
    }

    pub(in crate::environment_shell) fn rename(&mut self, old: &KraddieId, new: &KraddieId) {
        if let Some(mind) = self.minds.remove(old) {
            self.minds.insert(new.clone(), mind);
        }
        if let Some(activity) = self.activity.remove(old) {
            self.activity.insert(new.clone(), activity);
        }
        if let Some(at) = self.last_check_in.remove(old) {
            self.last_check_in.insert(new.clone(), at);
        }
        self.revision = self.revision.wrapping_add(1);
    }

    pub(in crate::environment_shell) fn forget(&mut self, id: &KraddieId) {
        self.minds.remove(id);
        self.activity.remove(id);
        self.last_check_in.remove(id);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct KraddieDigests {
    pub(in crate::environment_shell) ordinary: DigestBook,
    pub(in crate::environment_shell) books: HashMap<KraddieId, DigestBook>,
}

impl KraddieDigests {
    pub(in crate::environment_shell) fn book(&self, owner: &MemoryOwner) -> Option<&DigestBook> {
        match owner {
            MemoryOwner::Ordinary => Some(&self.ordinary),
            MemoryOwner::Kraddie(id) => self.books.get(id),
        }
    }

    pub(in crate::environment_shell) fn book_mut(
        &mut self,
        owner: &MemoryOwner,
    ) -> &mut DigestBook {
        match owner {
            MemoryOwner::Ordinary => &mut self.ordinary,
            MemoryOwner::Kraddie(id) => self.books.entry(id.clone()).or_default(),
        }
    }

    pub(in crate::environment_shell) fn replace(&mut self, digests: &[OwnedDigests]) {
        self.ordinary = DigestBook::default();
        self.books.clear();
        for owned in digests {
            self.adopt(owned);
        }
    }

    pub(in crate::environment_shell) fn adopt(&mut self, owned: &OwnedDigests) {
        self.book_mut(&owned.owner).clone_from(&owned.book);
    }

    pub(in crate::environment_shell) fn rename(&mut self, old: &KraddieId, new: &KraddieId) {
        if let Some(book) = self.books.remove(old) {
            self.books.insert(new.clone(), book);
        }
    }

    pub(in crate::environment_shell) fn forget(&mut self, id: &KraddieId) {
        self.books.remove(id);
    }
}
