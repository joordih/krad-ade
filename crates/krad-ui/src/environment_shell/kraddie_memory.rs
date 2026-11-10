mod area;
mod bus;
mod chats;
mod digests;
mod layout;
mod mention;
mod sharing;
mod sidebar;

use gpui_kit::component::input::TextareaState;
use gpui_kit::{AppContext as _, Context, Entity, Task};
use krad_chat::LiveChat;
use krad_domain::{
    chat_pack, memory_reach, DigestBook, DigestId, EnvironmentName, KraddieChip, KraddieId,
    KraddieMood, MemoryKind, MemoryLibrary, MemoryOwner, MemoryReach, MemorySize, PackRequest,
    SessionId, TerminalId, CHAT_PACK_TOKENS,
};
use krad_store::{KraddieShelf, OrdinaryDigests};
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

pub(super) struct KraddieMemoryUi {
    ordinary: DigestBook,
    books: HashMap<KraddieId, DigestBook>,
    loaded: HashSet<MemoryOwner>,
    loading: HashSet<MemoryOwner>,
    ordinary_store: Option<OrdinaryDigests>,
    shelf: Option<KraddieShelf>,
    quiet: HashMap<ChatKey, Instant>,
    writing: HashMap<ChatKey, LiveChat>,
    attempted: HashMap<ChatKey, usize>,
    pub(super) expanded: HashSet<KraddieId>,
    older_digests: HashMap<KraddieId, usize>,
    pub(super) chips: HashMap<TerminalId, Vec<KraddieChip>>,
    editing: Option<DigestEdit>,
    ticker: Option<Task<()>>,
    saving: HashMap<MemoryOwner, SaveTurn>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveTurn {
    Writing,
    WritingThenAgain,
}

fn ask_save(saving: &mut HashMap<MemoryOwner, SaveTurn>, owner: &MemoryOwner) -> bool {
    if let Some(turn) = saving.get_mut(owner) {
        *turn = SaveTurn::WritingThenAgain;
        return false;
    }
    saving.insert(owner.clone(), SaveTurn::Writing);
    true
}

fn saved(saving: &mut HashMap<MemoryOwner, SaveTurn>, owner: &MemoryOwner) -> bool {
    if saving.get(owner) == Some(&SaveTurn::WritingThenAgain) {
        saving.insert(owner.clone(), SaveTurn::Writing);
        return true;
    }
    saving.remove(owner);
    false
}

impl Default for KraddieMemoryUi {
    fn default() -> Self {
        let ordinary_store = OrdinaryDigests::in_config()
            .inspect_err(|error| tracing::debug!(%error, "no folder for chat digests"))
            .ok();
        Self {
            ordinary: DigestBook::default(),
            books: HashMap::new(),
            loaded: HashSet::new(),
            loading: HashSet::new(),
            ordinary_store,
            shelf: KraddieShelf::in_config(),
            quiet: HashMap::new(),
            writing: HashMap::new(),
            attempted: HashMap::new(),
            expanded: HashSet::new(),
            older_digests: HashMap::new(),
            chips: HashMap::new(),
            editing: None,
            ticker: None,
            saving: HashMap::new(),
        }
    }
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
        self.load_missing_digests(cx);
        self.check_digests(cx);
    }

    fn load_missing_digests(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<MemoryOwner> = std::iter::once(MemoryOwner::Ordinary)
            .chain(
                self.kraddies
                    .entries
                    .iter()
                    .map(|entry| MemoryOwner::Kraddie(entry.kraddie.id())),
            )
            .filter(|owner| {
                !self.kraddies.memory.loaded.contains(owner)
                    && !self.kraddies.memory.loading.contains(owner)
            })
            .collect();
        if wanted.is_empty() {
            return;
        }
        self.kraddies.memory.loading.extend(wanted.iter().cloned());
        let ordinary_files = self.kraddies.memory.ordinary_store.clone();
        let kraddie_files = self.kraddies.memory.shelf.clone();
        let read = cx.background_spawn(async move {
            wanted
                .into_iter()
                .map(|owner| {
                    let book = match &owner {
                        MemoryOwner::Ordinary => ordinary_files.as_ref().map(OrdinaryDigests::read),
                        MemoryOwner::Kraddie(id) => {
                            kraddie_files.as_ref().map(|files| files.read_digests(id))
                        }
                    };
                    let book = match book {
                        Some(Ok(book)) => book,
                        Some(Err(error)) => {
                            tracing::warn!(%owner, %error, "chat digests unreadable; set aside");
                            DigestBook::default()
                        }
                        None => DigestBook::default(),
                    };
                    (owner, book)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let books = read.await;
            this.update(cx, |shell, cx| {
                for (owner, book) in books {
                    shell.kraddies.memory.loading.remove(&owner);
                    *shell.kraddies.memory.book_mut(&owner) = book;
                    shell.kraddies.memory.loaded.insert(owner);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn save_digests(&mut self, owner: &MemoryOwner, cx: &mut Context<Self>) {
        if ask_save(&mut self.kraddies.memory.saving, owner) {
            self.write_digests(owner.clone(), cx);
        }
    }

    fn write_digests(&mut self, owner: MemoryOwner, cx: &mut Context<Self>) {
        let Some(book) = self.kraddies.memory.book(&owner).cloned() else {
            self.kraddies.memory.saving.remove(&owner);
            return;
        };
        let ordinary_files = self.kraddies.memory.ordinary_store.clone();
        let kraddie_files = self.kraddies.memory.shelf.clone();
        let target = owner.clone();
        let written = cx.background_spawn(async move {
            let written = match &target {
                MemoryOwner::Ordinary => ordinary_files.map(|store| store.write(&book)),
                MemoryOwner::Kraddie(id) => {
                    kraddie_files.map(|files| files.write_digests(id, &book))
                }
            };
            if let Some(Err(error)) = written {
                tracing::warn!(owner = %target, %error, "chat digests not saved");
            }
        });
        cx.spawn(async move |this, cx| {
            written.await;
            this.update(cx, |shell, cx| {
                if saved(&mut shell.kraddies.memory.saving, &owner) {
                    shell.write_digests(owner, cx);
                }
            })
            .unwrap_or_default();
        })
        .detach();
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

    #[test]
    fn one_digest_write_runs_per_owner_and_a_change_meanwhile_writes_again() {
        let mut saving = HashMap::new();
        let ordinary = MemoryOwner::Ordinary;
        let kraddie = MemoryOwner::Kraddie(
            KraddieId::try_from("a".to_owned()).unwrap_or_else(|_| unreachable!("an id")),
        );
        assert!(ask_save(&mut saving, &ordinary));
        assert!(ask_save(&mut saving, &kraddie));
        assert!(!ask_save(&mut saving, &ordinary));
        assert!(!ask_save(&mut saving, &ordinary));
        assert!(saved(&mut saving, &ordinary));
        assert!(!ask_save(&mut saving, &ordinary));
        assert!(saved(&mut saving, &ordinary));
        assert!(!saved(&mut saving, &ordinary));
        assert!(!saved(&mut saving, &kraddie));
        assert!(saving.is_empty());
        assert!(ask_save(&mut saving, &ordinary));
    }
}
