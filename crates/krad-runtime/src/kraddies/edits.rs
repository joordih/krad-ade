use krad_domain::{
    Automation, AutomationKey, DigestDraft, DigestId, Kraddie, KraddieCompass, KraddieHistory,
    KraddieId, KraddieMemory, MemoryError, MemoryOwner, Switch,
};
use krad_protocol::{AutomationState, ErrorCode, KraddieState, Reply};

use super::writer::{Renamed, ShelfWrite};
use super::{Dirty, Kraddies};
use crate::runtime::refused;

pub(crate) const NAME_TAKEN: &str = "A Kraddie already has that name";
const NO_KRADDIE: &str = "No Kraddie with that name";
const NO_AUTOMATION: &str = "No automation with that name";
const AUTOMATION_TAKEN: &str = "That automation number is taken; save it again";

pub(crate) struct Saved {
    pub(crate) renamed: Option<KraddieId>,
}

impl Kraddies {
    pub(crate) fn save(
        &mut self,
        kraddie: Kraddie,
        replaced: Option<&KraddieId>,
    ) -> Result<Saved, Reply> {
        let id = kraddie.id();
        let renamed = replaced.filter(|old| **old != id).cloned();
        let taken = self.state(&id).is_some();
        if taken && replaced.is_none_or(|old| *old != id) {
            return Err(refused(ErrorCode::Invalid, NAME_TAKEN));
        }
        let Some(old) = renamed else {
            if let Some(state) = self.state_mut(&id) {
                state.replace(kraddie);
            } else {
                self.states.push(KraddieState::new(kraddie));
                self.changed.insert(id.clone());
                self.sort();
            }
            self.dirty.insert(Dirty::Kraddie(id.clone()));
            self.dirty.insert(Dirty::Automations(id));
            return Ok(Saved { renamed: None });
        };
        let Some(at) = self.states.iter().position(|state| state.id() == old) else {
            return Err(refused(ErrorCode::NotFound, NO_KRADDIE));
        };
        let mut state = self.states.remove(at);
        state.replace(kraddie);
        self.limiter.end_kraddie(&old);
        self.rename_readers(&old, &id);
        if let Some(book) = self.books.remove(&old) {
            self.books.insert(id.clone(), book);
        }
        self.write(ShelfWrite::Rename(Box::new(Renamed {
            old: old.clone(),
            kraddie: state.kraddie.clone(),
            automations: state
                .automations
                .iter()
                .map(|automation| automation.automation.clone())
                .collect(),
            histories: state
                .automations
                .iter()
                .map(|automation| {
                    let key = AutomationKey::new(id.clone(), automation.automation.id);
                    (key, automation.history.clone())
                })
                .collect(),
        })));
        self.forget_dirt(&old);
        self.changed.remove(&old);
        self.removed.insert(old.clone());
        self.states.push(state);
        self.changed.insert(id);
        self.sort();
        Ok(Saved { renamed: Some(old) })
    }

    fn rename_readers(&mut self, old: &KraddieId, new: &KraddieId) {
        let readers: Vec<KraddieId> = self
            .states
            .iter_mut()
            .filter_map(|state| {
                let reads = &mut state.kraddie.settings.reads;
                let at = reads.iter().position(|read| read == old)?;
                reads[at] = new.clone();
                Some(state.id())
            })
            .collect();
        for reader in readers {
            self.changed.insert(reader.clone());
            self.dirty.insert(Dirty::Kraddie(reader));
        }
    }

    fn forget_dirt(&mut self, id: &KraddieId) {
        self.dirty.retain(|dirty| match dirty {
            Dirty::Kraddie(owner)
            | Dirty::Automations(owner)
            | Dirty::Mind(owner)
            | Dirty::Activity(owner) => owner != id,
            Dirty::History(key) => key.kraddie != *id,
            Dirty::Digests(owner) => owner.kraddie() != Some(id),
        });
    }

    pub(crate) fn import(&mut self, kraddie: Kraddie, automations: Vec<Automation>) -> Reply {
        let id = kraddie.id();
        if self.state(&id).is_some() {
            return refused(ErrorCode::Invalid, NAME_TAKEN);
        }
        let mut state = KraddieState::new(kraddie);
        state.automations = automations
            .into_iter()
            .map(|automation| AutomationState {
                automation,
                history: KraddieHistory::default(),
            })
            .collect();
        self.states.push(state);
        self.sort();
        self.changed.insert(id.clone());
        self.dirty.insert(Dirty::Kraddie(id.clone()));
        self.dirty.insert(Dirty::Automations(id));
        Reply::Accepted
    }

    pub(crate) fn delete(&mut self, id: &KraddieId) -> Reply {
        let Some(at) = self.states.iter().position(|state| state.id() == *id) else {
            return refused(ErrorCode::NotFound, NO_KRADDIE);
        };
        self.states.remove(at);
        self.books.remove(id);
        self.limiter.end_kraddie(id);
        self.changed.remove(id);
        self.forget_dirt(id);
        self.write(ShelfWrite::Delete(id.clone()));
        self.removed.insert(id.clone());
        tracing::info!(kraddie = %id, "kraddie deleted");
        Reply::Accepted
    }

    pub(crate) fn save_automation(
        &mut self,
        kraddie: &KraddieId,
        automation: Automation,
        previous: Option<&AutomationKey>,
    ) -> Reply {
        let stays = previous.is_some_and(|key| key.kraddie == *kraddie);
        let Some(state) = self.state(kraddie) else {
            return refused(ErrorCode::NotFound, NO_KRADDIE);
        };
        let replaces_itself = stays && previous.is_some_and(|key| key.automation == automation.id);
        if state.automation(automation.id).is_some() && !replaces_itself {
            return refused(ErrorCode::Invalid, AUTOMATION_TAKEN);
        }
        let history = previous
            .and_then(|key| self.take_automation(key, !stays))
            .unwrap_or_default();
        let key = AutomationKey::new(kraddie.clone(), automation.id);
        let Some(state) = self.state_mut(kraddie) else {
            return refused(ErrorCode::NotFound, NO_KRADDIE);
        };
        let at = state
            .automations
            .iter()
            .position(|known| known.automation.id >= automation.id)
            .unwrap_or(state.automations.len());
        state.automations.insert(
            at,
            AutomationState {
                automation,
                history,
            },
        );
        self.dirty.insert(Dirty::Automations(kraddie.clone()));
        self.dirty.insert(Dirty::History(key));
        Reply::Accepted
    }

    fn take_automation(&mut self, key: &AutomationKey, forget: bool) -> Option<KraddieHistory> {
        let state = self.state_mut(&key.kraddie)?;
        let at = state
            .automations
            .iter()
            .position(|known| known.automation.id == key.automation)?;
        let taken = state.automations.remove(at);
        self.dirty.insert(Dirty::Automations(key.kraddie.clone()));
        if forget {
            self.dirty.remove(&Dirty::History(key.clone()));
            self.write(ShelfWrite::ForgetHistory(key.clone()));
        }
        Some(taken.history)
    }

    pub(crate) fn delete_automation(&mut self, key: &AutomationKey) -> Reply {
        self.limiter.end(key);
        match self.take_automation(key, true) {
            Some(_) => Reply::Accepted,
            None => refused(ErrorCode::NotFound, NO_AUTOMATION),
        }
    }

    pub(crate) fn switch_automation(&mut self, key: &AutomationKey, switch: Switch) -> Reply {
        let Some(automation) = self
            .state_mut(&key.kraddie)
            .and_then(|state| state.automation_mut(key.automation))
        else {
            return refused(ErrorCode::NotFound, NO_AUTOMATION);
        };
        automation.automation.switch = switch;
        self.dirty.insert(Dirty::Automations(key.kraddie.clone()));
        Reply::Accepted
    }

    pub(crate) fn set_compass(&mut self, id: &KraddieId, compass: KraddieCompass) -> Reply {
        let Some(state) = self.state_mut(id) else {
            return refused(ErrorCode::NotFound, NO_KRADDIE);
        };
        state.mind.compass = compass;
        self.dirty.insert(Dirty::Mind(id.clone()));
        Reply::Accepted
    }

    pub(crate) fn edit_memory(
        &mut self,
        id: &KraddieId,
        edit: impl FnOnce(&mut KraddieMemory) -> Result<(), MemoryError>,
    ) -> Reply {
        let Some(state) = self.state_mut(id) else {
            return refused(ErrorCode::NotFound, NO_KRADDIE);
        };
        if let Err(error) = edit(&mut state.mind.memory) {
            let code = match error {
                MemoryError::NoSuchLine => ErrorCode::NotFound,
                MemoryError::Empty | MemoryError::TooLong | MemoryError::Full => ErrorCode::Invalid,
            };
            return refused(code, &error.to_string());
        }
        self.dirty.insert(Dirty::Mind(id.clone()));
        Reply::Accepted
    }

    pub(crate) fn reset_conversation(&mut self, id: &KraddieId) -> Reply {
        let Some(state) = self.state_mut(id) else {
            return refused(ErrorCode::NotFound, NO_KRADDIE);
        };
        state.check_ins = KraddieHistory::default();
        state.last_check_in = None;
        let keys: Vec<AutomationKey> = state
            .automations
            .iter_mut()
            .map(|automation| {
                automation.history = KraddieHistory::default();
                AutomationKey::new(id.clone(), automation.automation.id)
            })
            .collect();
        self.dirty.extend(keys.into_iter().map(Dirty::History));
        tracing::info!(kraddie = %id, "kraddie conversation reset");
        Reply::Accepted
    }

    pub(crate) fn revise_digest(
        &mut self,
        owner: &MemoryOwner,
        digest: DigestId,
        text: &str,
    ) -> Reply {
        match self.book_mut(owner).edit(digest, text) {
            Ok(()) => Reply::Accepted,
            Err(error) => refused(ErrorCode::Invalid, &error.to_string()),
        }
    }

    pub(crate) fn forget_digest(&mut self, owner: &MemoryOwner, digest: DigestId) -> Reply {
        match self.book_mut(owner).remove(digest) {
            Ok(_) => {
                tracing::info!(%owner, digest = digest.0, "chat digest forgotten");
                Reply::Accepted
            }
            Err(error) => refused(ErrorCode::NotFound, &error.to_string()),
        }
    }

    pub(crate) fn record_digest(&mut self, owner: &MemoryOwner, draft: DigestDraft) -> bool {
        let kept = self.book_mut(owner).record(draft).is_some();
        if kept {
            tracing::info!(%owner, "chat digest kept");
        }
        kept
    }
}

impl Kraddies {
    pub(crate) fn share_memory(&mut self, reader: &KraddieId, from: &KraddieId, switch: Switch) {
        let Some(state) = self.state_mut(reader) else {
            return;
        };
        let changed = match switch {
            Switch::On => state.kraddie.settings.share_from(reader, from.clone()),
            Switch::Off => state.kraddie.settings.unshare_from(from),
        };
        if changed {
            self.dirty.insert(Dirty::Kraddie(reader.clone()));
            tracing::info!(reader = %reader, switch = switch.display_name(), "kraddie memory sharing changed");
        }
    }

    pub(crate) fn remember(&mut self, id: &KraddieId, text: &str) -> Result<(), MemoryError> {
        let Some(state) = self.state_mut(id) else {
            return Err(MemoryError::NoSuchLine);
        };
        state.mind.memory.add(text)?;
        self.dirty.insert(Dirty::Mind(id.clone()));
        tracing::info!(kraddie = %id, "kraddie remembered a line");
        Ok(())
    }
}
