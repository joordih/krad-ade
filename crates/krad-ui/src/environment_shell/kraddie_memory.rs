mod area;
mod bus;
mod chats;
mod layout;
mod mention;
mod sharing;
mod sidebar;

use gpui_kit::component::input::TextareaState;
use gpui_kit::Entity;
use krad_domain::{
    DigestBook, DigestId, EnvironmentName, KraddieChip, KraddieId, KraddieMood, MemoryLibrary,
    MemoryOwner, MemorySize, SessionId, TerminalId,
};
use krad_protocol::OwnedDigests;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::EnvironmentShell;
use crate::kraddie_avatar::KraddieSprite;

pub(super) use mention::KraddieSuggestion;

pub(super) struct DigestEdit {
    owner: MemoryOwner,
    digest: DigestId,
    text: Entity<TextareaState>,
}

#[derive(Default)]
pub(super) struct KraddieMemoryUi {
    ordinary: DigestBook,
    books: HashMap<KraddieId, DigestBook>,
    known: bool,
    pub(super) expanded: HashSet<KraddieId>,
    older_digests: HashMap<KraddieId, usize>,
    pub(super) chips: HashMap<TerminalId, Vec<KraddieChip>>,
    editing: Option<DigestEdit>,
}

impl KraddieMemoryUi {
    fn book(&self, owner: &MemoryOwner) -> Option<&DigestBook> {
        match owner {
            MemoryOwner::Ordinary => Some(&self.ordinary),
            MemoryOwner::Kraddie(id) => self.books.get(id),
        }
    }

    fn book_mut(&mut self, owner: &MemoryOwner) -> &mut DigestBook {
        match owner {
            MemoryOwner::Ordinary => &mut self.ordinary,
            MemoryOwner::Kraddie(id) => self.books.entry(id.clone()).or_default(),
        }
    }

    pub(super) fn replace_books(&mut self, digests: Vec<OwnedDigests>) {
        self.ordinary = DigestBook::default();
        self.books.clear();
        for owned in digests {
            self.adopt_book(owned);
        }
        self.known = true;
    }

    pub(super) fn adopt_book(&mut self, owned: OwnedDigests) {
        *self.book_mut(&owned.owner) = owned.book;
    }

    pub(super) fn forget_book(&mut self, id: &KraddieId) {
        self.books.remove(id);
        self.expanded.remove(id);
    }
}

impl EnvironmentShell {
    pub(super) fn memory_library(&self) -> MemoryLibrary {
        let mut library = MemoryLibrary::default();
        for entry in &self.runtime.kraddies.entries {
            let id = entry.kraddie.id();
            library.add_keepsakes(&id, &self.kraddie_mind(&id).memory);
            if let Some(book) = self.kraddies.memory.books.get(&id) {
                library.add_digests(&MemoryOwner::Kraddie(id), book);
            }
        }
        library.add_digests(&MemoryOwner::Ordinary, &self.kraddies.memory.ordinary);
        library
    }

    pub(super) fn kraddie_chat_look(
        &self,
        env: &EnvironmentName,
        session: SessionId,
    ) -> Option<Rc<KraddieSprite>> {
        let id = self
            .envs
            .get(env)?
            .data
            .session(session)?
            .memory
            .kraddie
            .clone()?;
        let entry = self.runtime.kraddies.entry(&id)?;
        Some(self.kraddies.sprite(entry, KraddieMood::Idle))
    }

    pub(super) fn memory_tools_offered(&self) -> bool {
        !self.runtime.kraddies.entries.is_empty() || !self.kraddies.memory.ordinary.is_empty()
    }

    pub(super) fn memory_size(&self, kraddie: &KraddieId) -> MemorySize {
        MemorySize {
            keepsakes: self.kraddie_mind(kraddie).memory.len(),
            digests: self
                .kraddies
                .memory
                .books
                .get(kraddie)
                .map_or(0, DigestBook::len),
        }
    }
}
