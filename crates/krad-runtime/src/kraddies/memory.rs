use krad_agents::{
    memory_hit_limit, ApiError, DigestInfo, ErrorCode, KraddieMemoryInfo, KraddieRecallInfo,
    MemoryEntryInfo, MemoryHitInfo, Request, Response,
};
use krad_domain::{
    chat_pack, memory_reach, EnvironmentData, KraddieId, MemoryAccessError, MemoryKind,
    MemoryLibrary, MemoryOwner, MemoryReach, MemoryRef, PackRequest, SessionMemory, TerminalId,
    CHAT_PACK_TOKENS, MAX_MEMORY_LINES,
};

use krad_protocol::KraddieState;

use super::Kraddies;

const NOT_A_MEMORY_REQUEST: &str = "Not a memory request";
const NO_KRADDIE: &str = "No Kraddie with that name";
const CHATS_CANNOT_EDIT_MEMORY: &str = "Only you change what a Kraddie remembers or reads, from your own terminal or in the app; a chat cannot";
const OUT_OF_REACH: &str =
    "This chat may not read that Kraddie's memory unless its Kraddie reads it or you mention it with $";
const PROMPT_SEPARATOR: &str = "\n\n";

fn access_code(error: MemoryAccessError) -> ErrorCode {
    match error {
        MemoryAccessError::NotFound => ErrorCode::NotFound,
        MemoryAccessError::NotShared => ErrorCode::Forbidden,
    }
}

fn joined(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}{PROMPT_SEPARATOR}{second}")),
        (first, second) => first.or(second),
    }
}

pub(crate) fn chat_owner(data: Option<&EnvironmentData>, chat: TerminalId) -> MemoryOwner {
    data.and_then(|data| data.chat_memory(chat))
        .map_or(MemoryOwner::Ordinary, SessionMemory::owner)
}

impl Kraddies {
    pub(crate) fn offers_memory_tools(&self) -> bool {
        !self.states.is_empty() || !self.ordinary.is_empty()
    }

    pub(crate) fn kraddie_named(&self, name: &str) -> Option<KraddieId> {
        self.states
            .iter()
            .find(|state| state.kraddie.name.matches(name) || state.id().as_str() == name)
            .map(KraddieState::id)
    }

    pub(crate) fn library(&self) -> MemoryLibrary {
        let mut library = MemoryLibrary::default();
        for state in &self.states {
            let id = state.id();
            library.add_keepsakes(&id, &state.mind.memory);
            if let Some(book) = self.books.get(&id) {
                library.add_digests(&MemoryOwner::Kraddie(id), book);
            }
        }
        library.add_digests(&MemoryOwner::Ordinary, &self.ordinary);
        library
    }

    pub(crate) fn reach(
        &self,
        data: Option<&EnvironmentData>,
        chat: Option<TerminalId>,
    ) -> MemoryReach {
        let kraddies = self.states.iter().map(|state| &state.kraddie);
        let Some(chat) = chat else {
            return memory_reach(&MemoryOwner::Ordinary, kraddies, &[]);
        };
        let memory = data.and_then(|data| data.chat_memory(chat));
        let owner = memory.map_or(MemoryOwner::Ordinary, SessionMemory::owner);
        let mentioned = memory.map_or(&[][..], |memory| memory.mentioned_in(chat));
        memory_reach(&owner, kraddies, mentioned)
    }

    pub(crate) fn preamble(&self, data: &EnvironmentData, chat: TerminalId) -> Option<String> {
        let owner = chat_owner(Some(data), chat);
        let identity = owner.kraddie().and_then(|id| {
            let state = self.state(id)?;
            Some(state.mind.chat_brief(&state.kraddie))
        });
        let reach = self.reach(Some(data), Some(chat));
        let library = self.library();
        let query = data
            .session_of(chat)
            .and_then(|id| data.session(id))
            .map_or_default(|session| session.name.clone());
        let request = PackRequest {
            home: &owner,
            query: &query,
            budget: CHAT_PACK_TOKENS,
            now: self.now().at,
        };
        let visible = library
            .visible(&reach)
            .filter(|item| !(item.kind == MemoryKind::Keepsake && item.owner() == owner));
        let pack = chat_pack(visible, &request);
        let pack = (!pack.is_empty()).then_some(pack.text);
        joined(identity, pack)
    }

    pub(crate) fn memory_refusal(
        &self,
        request: &Request,
        chat: Option<&MemoryReach>,
    ) -> Option<ApiError> {
        let reach = chat?;
        match request {
            Request::RememberKraddie { .. } | Request::ShareKraddieMemory { .. } => Some(
                ApiError::new(ErrorCode::Forbidden, CHATS_CANNOT_EDIT_MEMORY),
            ),
            Request::KraddieMemory { name } | Request::KraddieRecall { name, .. } => {
                let owner = MemoryOwner::Kraddie(self.kraddie_named(name)?);
                (!reach.allows(&owner)).then(|| ApiError::new(ErrorCode::OutOfReach, OUT_OF_REACH))
            }
            _ => None,
        }
    }

    pub(crate) fn memory_info(&self, id: &KraddieId) -> Option<KraddieMemoryInfo> {
        Some(KraddieMemoryInfo {
            kraddie: id.to_string(),
            lines: self.state(id)?.mind.memory.lines().to_vec(),
            limit: MAX_MEMORY_LINES,
        })
    }

    pub(crate) fn recall(&self, name: &str) -> Response {
        let Some(state) = self.kraddie_named(name).and_then(|id| self.state(&id)) else {
            return Response::error(ErrorCode::NotFound, format!("{NO_KRADDIE}: {name}"));
        };
        let owner = MemoryOwner::Kraddie(state.id());
        let digests = self.book(&owner).map_or_default(|book| {
            book.digests()
                .iter()
                .rev()
                .map(|digest| DigestInfo {
                    id: MemoryRef::Digest {
                        owner: owner.clone(),
                        digest: digest.id,
                    }
                    .to_string(),
                    chat: digest.chat.title.clone(),
                    facts: digest.facts.clone(),
                })
                .collect()
        });
        Response::KraddieRecall(Box::new(KraddieRecallInfo {
            kraddie: state.id().to_string(),
            keepsakes: state.mind.memory.lines().to_vec(),
            digests,
            reads: state
                .kraddie
                .settings
                .reads
                .iter()
                .map(ToString::to_string)
                .collect(),
            automatic_digests: state.kraddie.settings.digests.is_automatic(),
        }))
    }
}

pub(crate) fn memory_answer(
    library: &MemoryLibrary,
    reach: &MemoryReach,
    request: &Request,
) -> Response {
    match request {
        Request::MemorySearch { query, limit } => Response::MemoryHits(
            library
                .search(reach, query, memory_hit_limit(*limit))
                .into_iter()
                .map(|hit| MemoryHitInfo {
                    id: hit.reference.to_string(),
                    title: hit.title,
                    excerpt: hit.excerpt,
                })
                .collect(),
        ),
        Request::MemoryRead { id } => {
            let reference = match id.parse::<MemoryRef>() {
                Ok(reference) => reference,
                Err(error) => return Response::error(ErrorCode::BadRequest, error.to_string()),
            };
            match library.read(reach, &reference) {
                Ok(item) => Response::MemoryEntry(Box::new(MemoryEntryInfo {
                    id: item.reference.to_string(),
                    title: item.title.clone(),
                    text: item.text.clone(),
                })),
                Err(error) => Response::error(access_code(error), error.to_string()),
            }
        }
        _ => Response::error(ErrorCode::BadRequest, NOT_A_MEMORY_REQUEST),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{
        AgentPreset, DigestChat, DigestDraft, EnvironmentName, Kraddie, KraddieMemory, SessionId,
        Switch,
    };
    use std::time::SystemTime;

    fn id(text: &str) -> KraddieId {
        KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!())
    }

    fn kraddies() -> Kraddies {
        let (inbox, _) = std::sync::mpsc::sync_channel(1);
        let mut kraddies = Kraddies::start(super::super::KraddiesSetup::default(), inbox);
        for name in ["A", "B"] {
            let kraddie = Kraddie::new(
                name.parse().unwrap_or_else(|_| unreachable!()),
                AgentPreset::ClaudeCode,
            );
            assert!(kraddies.save(kraddie, None).is_ok());
        }
        let mut memory = KraddieMemory::default();
        assert!(memory
            .add("B keeps the staging password rotation notes")
            .is_ok());
        let mind = krad_domain::KraddieMind {
            memory,
            ..krad_domain::KraddieMind::default()
        };
        assert_eq!(
            kraddies.set_mind(&id("b"), mind),
            krad_protocol::Reply::Accepted
        );
        kraddies.record_digest(
            &MemoryOwner::Ordinary,
            DigestDraft {
                chat: DigestChat {
                    environment: EnvironmentName::new("main".to_owned()),
                    session: SessionId(1),
                    terminal: TerminalId(1),
                    title: "Staging".to_owned(),
                    folder: None,
                    resume: None,
                },
                facts: vec!["Staging deploys from the main branch".to_owned()],
                covered: 2,
                written: SystemTime::UNIX_EPOCH,
            },
        );
        kraddies
    }

    fn reach_of(kraddies: &Kraddies, mentioned: &[KraddieId]) -> MemoryReach {
        memory_reach(
            &MemoryOwner::Kraddie(id("a")),
            kraddies.states().iter().map(|state| &state.kraddie),
            mentioned,
        )
    }

    #[test]
    fn a_chat_reads_only_the_kraddies_in_its_reach_and_never_edits_memory() {
        let kraddies = kraddies();
        let a = reach_of(&kraddies, &[]);
        let mentioned = reach_of(&kraddies, &[id("b")]);
        let code = |request: &Request, reach: Option<&MemoryReach>| {
            kraddies
                .memory_refusal(request, reach)
                .map(|error| error.code)
        };
        let read_b = Request::KraddieRecall {
            name: "B".to_owned(),
            view: krad_agents::RecallView::default(),
        };
        assert_eq!(code(&read_b, Some(&a)), Some(ErrorCode::OutOfReach));
        assert_eq!(code(&read_b, Some(&mentioned)), None);
        assert_eq!(code(&read_b, None), None);
        let own = Request::KraddieMemory {
            name: "A".to_owned(),
        };
        assert_eq!(code(&own, Some(&a)), None);
        let share = Request::ShareKraddieMemory {
            reader: "A".to_owned(),
            from: "B".to_owned(),
            switch: Switch::On,
        };
        assert_eq!(code(&share, Some(&mentioned)), Some(ErrorCode::Forbidden));
        assert_eq!(code(&share, None), None);
    }

    #[test]
    fn the_bus_enforces_the_rules_on_every_memory_call() {
        let kraddies = kraddies();
        let library = kraddies.library();
        let a = reach_of(&kraddies, &[]);
        let search = Request::MemorySearch {
            query: "staging".to_owned(),
            limit: None,
        };
        let Response::MemoryHits(hits) = memory_answer(&library, &a, &search) else {
            unreachable!("a search answers with hits");
        };
        assert_eq!(
            hits.iter().map(|hit| hit.id.as_str()).collect::<Vec<_>>(),
            ["chats/digest/1"]
        );
        let read = Request::MemoryRead {
            id: "kraddie/b/keepsake/1".to_owned(),
        };
        assert!(matches!(
            memory_answer(&library, &a, &read),
            Response::Error(error) if error.code == ErrorCode::Forbidden
        ));
        let mentioned = reach_of(&kraddies, &[id("b")]);
        assert!(matches!(
            memory_answer(&library, &mentioned, &read),
            Response::MemoryEntry(entry) if entry.text.contains("rotation")
        ));
        assert!(kraddies.offers_memory_tools());
        let (inbox, _) = std::sync::mpsc::sync_channel(1);
        assert!(
            !Kraddies::start(super::super::KraddiesSetup::default(), inbox).offers_memory_tools()
        );
    }
}
