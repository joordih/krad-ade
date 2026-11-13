mod area;
mod bus;
mod chats;
mod digests;
mod layout;
mod mention;
mod sharing;
mod sidebar;

use gpui_kit::component::input::TextareaState;
use gpui_kit::{Context, Entity, Task};
use krad_chat::LiveChat;
use krad_domain::{
    chat_pack, memory_reach, DigestBook, DigestId, EnvironmentName, KraddieChip, KraddieId,
    KraddieMood, MemoryKind, MemoryLibrary, MemoryOwner, MemoryReach, MemorySize, PackRequest,
    SessionId, TerminalId, CHAT_PACK_TOKENS,
};
use krad_protocol::OwnedDigests;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use super::EnvironmentShell;
use crate::kraddie_avatar::KraddieSprite;

pub(super) use mention::KraddieSuggestion;

const PROMPT_SEPARATOR: &str = "\n\n";

type ChatKey = (EnvironmentName, TerminalId);

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
    quiet: HashMap<ChatKey, Instant>,
    writing: HashMap<ChatKey, Option<LiveChat>>,
    attempted: HashMap<ChatKey, usize>,
    pub(super) expanded: HashSet<KraddieId>,
    older_digests: HashMap<KraddieId, usize>,
    pub(super) chips: HashMap<TerminalId, Vec<KraddieChip>>,
    editing: Option<DigestEdit>,
    ticker: Option<Task<()>>,
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

#[must_use]
pub(super) fn joined_prompt(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}{PROMPT_SEPARATOR}{second}")),
        (first, second) => first.or(second),
    }
}

impl EnvironmentShell {
    pub(super) fn memory_library(&self) -> MemoryLibrary {
        let mut library = MemoryLibrary::default();
        for entry in &self.kraddies.entries {
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
        let sprite = self.kraddies.entry(&id)?.sprite(KraddieMood::Idle);
        Some(Rc::clone(sprite))
    }

    pub(super) fn memory_tools_offered(&self) -> bool {
        !self.kraddies.entries.is_empty() || !self.kraddies.memory.ordinary.is_empty()
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

    pub(super) fn chat_owner(&self, env: &EnvironmentName, chat: TerminalId) -> MemoryOwner {
        self.envs
            .get(env)
            .and_then(|live| live.data.chat_memory(chat))
            .map_or(MemoryOwner::Ordinary, krad_domain::SessionMemory::owner)
    }

    pub(super) fn chat_reach(&self, env: &EnvironmentName, chat: TerminalId) -> MemoryReach {
        let memory = self
            .envs
            .get(env)
            .and_then(|live| live.data.chat_memory(chat));
        let owner = memory.map_or(MemoryOwner::Ordinary, krad_domain::SessionMemory::owner);
        let mentioned = memory.map_or(&[][..], |memory| memory.mentioned_in(chat));
        memory_reach(
            &owner,
            self.kraddies.entries.iter().map(|entry| &entry.kraddie),
            mentioned,
        )
    }

    pub(super) fn memory_preamble(
        &self,
        env: &EnvironmentName,
        chat: TerminalId,
    ) -> Option<String> {
        let owner = self.chat_owner(env, chat);
        let identity = owner.kraddie().and_then(|id| {
            let entry = self.kraddies.entry(id)?;
            Some(self.kraddie_mind(id).chat_brief(&entry.kraddie))
        });
        let reach = self.chat_reach(env, chat);
        let library = self.memory_library();
        let query = self
            .envs
            .get(env)
            .and_then(|live| {
                live.data
                    .session_of(chat)
                    .and_then(|id| live.data.session(id))
            })
            .map_or_default(|session| session.name.clone());
        let request = PackRequest {
            home: &owner,
            query: &query,
            budget: CHAT_PACK_TOKENS,
            now: SystemTime::now(),
        };
        let visible = library
            .visible(&reach)
            .filter(|item| !(item.kind == MemoryKind::Keepsake && item.owner() == owner));
        let pack = chat_pack(visible, &request);
        let pack = (!pack.is_empty()).then_some(pack.text);
        joined_prompt(identity, pack)
    }

    pub(super) fn start_memory_clock(&mut self, cx: &mut Context<Self>) {
        if self.kraddies.memory.ticker.is_some() {
            return;
        }
        self.kraddies.memory.ticker = Some(cx.spawn(async move |this, cx| loop {
            let alive = this.update(cx, Self::tend_memory);
            if alive.is_err() {
                break;
            }
            cx.background_executor()
                .timer(digests::DIGEST_CHECK_EVERY)
                .await;
        }));
    }

    fn tend_memory(&mut self, cx: &mut Context<Self>) {
        self.check_digests(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_join_with_a_blank_line_and_skip_what_is_missing() {
        assert_eq!(
            joined_prompt(Some("a".to_owned()), Some("b".to_owned())).as_deref(),
            Some("a\n\nb")
        );
        assert_eq!(
            joined_prompt(None, Some("b".to_owned())).as_deref(),
            Some("b")
        );
        assert_eq!(
            joined_prompt(Some("a".to_owned()), None).as_deref(),
            Some("a")
        );
        assert_eq!(joined_prompt(None, None), None);
    }
}
