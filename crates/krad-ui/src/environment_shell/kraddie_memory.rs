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
    pub(super) expanded: HashSet<KraddieId>,
    older_digests: HashMap<KraddieId, usize>,
    pub(super) chips: HashMap<TerminalId, Vec<KraddieChip>>,
    editing: Option<DigestEdit>,
}

impl KraddieMemoryUi {
    pub(super) fn forget_book(&mut self, id: &KraddieId) {
        self.expanded.remove(id);
    }
}

impl EnvironmentShell {
    pub(super) fn memory_library(&self) -> MemoryLibrary {
        let mut library = MemoryLibrary::default();
        for entry in &self.runtime.kraddies.entries {
            let id = entry.kraddie.id();
            library.add_keepsakes(&id, &self.kraddie_mind(&id).memory);
            if let Some(book) = self.runtime.kraddies.digests.books.get(&id) {
                library.add_digests(&MemoryOwner::Kraddie(id), book);
            }
        }
        library.add_digests(
            &MemoryOwner::Ordinary,
            &self.runtime.kraddies.digests.ordinary,
        );
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
        !self.runtime.kraddies.entries.is_empty()
            || !self.runtime.kraddies.digests.ordinary.is_empty()
    }

    pub(super) fn memory_size(&self, kraddie: &KraddieId) -> MemorySize {
        MemorySize {
            keepsakes: self.kraddie_mind(kraddie).memory.len(),
            digests: self
                .runtime
                .kraddies
                .digests
                .books
                .get(kraddie)
                .map_or(0, DigestBook::len),
        }
    }
}
