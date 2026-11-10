use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::Context;
use krad_agents::{
    memory_hit_limit, ApiError, Caller, DigestInfo, ErrorCode, KraddieRecallInfo,
    KraddieSharingInfo, MemoryEntryInfo, MemoryHitInfo, Request, Response,
};
use krad_domain::{
    memory_reach, EnvironmentName, KraddieId, MemoryAccessError, MemoryLibrary, MemoryOwner,
    MemoryReach, MemoryRef, NodeKind, SessionId, Switch, TerminalId,
};

use super::super::EnvironmentShell;

const NOT_A_MEMORY_REQUEST: &str = "Not a memory request";
const OWN_MEMORY: &str = "A Kraddie always reads its own memory";
const CHATS_CANNOT_EDIT_MEMORY: &str = "Only you change what a Kraddie remembers or reads, from your own terminal or in the app; a chat cannot";
const OUT_OF_REACH: &str =
    "This chat may not read that Kraddie's memory unless its Kraddie reads it or you mention it with $";

fn access_code(error: MemoryAccessError) -> ErrorCode {
    match error {
        MemoryAccessError::NotFound => ErrorCode::NotFound,
        MemoryAccessError::NotShared => ErrorCode::Forbidden,
    }
}

#[must_use]
pub(super) fn memory_answer(
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
                Ok(item) => Response::MemoryEntry(MemoryEntryInfo {
                    id: item.reference.to_string(),
                    title: item.title.clone(),
                    text: item.text.clone(),
                }),
                Err(error) => Response::error(access_code(error), error.to_string()),
            }
        }
        _ => Response::error(ErrorCode::BadRequest, NOT_A_MEMORY_REQUEST),
    }
}

#[must_use]
pub(in crate::environment_shell) fn memory_refusal(
    request: &Request,
    chat: Option<&MemoryReach>,
    kraddie_named: impl Fn(&str) -> Option<KraddieId>,
) -> Option<ApiError> {
    let reach = chat?;
    match request {
        Request::RememberKraddie { .. } | Request::ShareKraddieMemory { .. } => Some(
            ApiError::new(ErrorCode::Forbidden, CHATS_CANNOT_EDIT_MEMORY),
        ),
        Request::KraddieMemory { name } | Request::KraddieRecall { name, .. } => {
            let owner = MemoryOwner::Kraddie(kraddie_named(name)?);
            (!reach.allows(&owner)).then(|| ApiError::new(ErrorCode::OutOfReach, OUT_OF_REACH))
        }
        _ => None,
    }
}

impl EnvironmentShell {
    fn calling_chat(&self, from: &Caller) -> Option<(EnvironmentName, TerminalId)> {
        let (environment, terminal) = match from {
            Caller::Terminal(handle) => (&handle.environment, TerminalId(handle.terminal)),
            Caller::Lead(lead) => {
                let env = EnvironmentName::new(lead.environment.clone());
                let terminal = self
                    .envs
                    .get(&env)?
                    .data
                    .lead(SessionId(lead.session))?
                    .terminal?;
                (&lead.environment, terminal)
            }
            Caller::Git(_) | Caller::Client => return None,
        };
        let env = EnvironmentName::new(environment.clone());
        let is_chat = self
            .envs
            .get(&env)?
            .data
            .nodes
            .iter()
            .any(|node| node.id == terminal && node.kind == NodeKind::Chat);
        is_chat.then_some((env, terminal))
    }

    pub(in crate::environment_shell) fn memory_gate(
        &self,
        from: &Caller,
        request: &Request,
    ) -> Option<ApiError> {
        let reach = self
            .calling_chat(from)
            .map(|(env, chat)| self.chat_reach(&env, chat));
        memory_refusal(request, reach.as_ref(), |name| {
            self.kraddie_by_name(name).ok()
        })
    }

    pub(in crate::environment_shell) fn answer_memory(
        &self,
        environment: &str,
        chat: Option<TerminalId>,
        request: &Request,
    ) -> Response {
        let env = EnvironmentName::new(environment.to_owned());
        let reach = chat.map_or_else(
            || {
                memory_reach(
                    &MemoryOwner::Ordinary,
                    self.kraddies.entries.iter().map(|entry| &entry.kraddie),
                    &[],
                )
            },
            |chat| self.chat_reach(&env, chat),
        );
        tracing::debug!(env = env.as_str(), reader = %reach.reader(), "memory asked");
        memory_answer(&self.memory_library(), &reach, request)
    }

    pub(in crate::environment_shell) fn answer_memory_call(
        &mut self,
        environment: &str,
        chat: Option<TerminalId>,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            request,
            Request::MemorySearch { .. } | Request::MemoryRead { .. }
        ) {
            let answer = self.answer_memory(environment, chat, &request);
            reply.send(answer).unwrap_or_default();
            return;
        }
        self.answer_kraddie_memory(request, reply, cx);
    }

    pub(in crate::environment_shell) fn answer_kraddie_memory(
        &mut self,
        request: Request,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let response = match request {
            Request::KraddieRecall { name, .. } => self.kraddie_recall(&name),
            Request::ShareKraddieMemory {
                reader,
                from,
                switch,
            } => self.share_from_bus(&reader, &from, switch, cx),
            Request::NewKraddieChat { name } => self.kraddie_chat_from_bus(&name, cx),
            _ => Response::error(ErrorCode::BadRequest, NOT_A_MEMORY_REQUEST),
        };
        reply.send(response).unwrap_or_default();
    }

    pub(super) fn kraddie_by_name(&self, name: &str) -> Result<KraddieId, Response> {
        self.kraddies
            .entries
            .iter()
            .find(|entry| entry.kraddie.name.matches(name))
            .map(|entry| entry.kraddie.id())
            .ok_or_else(|| Response::error(ErrorCode::NotFound, format!("No Kraddie named {name}")))
    }

    fn kraddie_recall(&self, name: &str) -> Response {
        let id = match self.kraddie_by_name(name) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let Some(entry) = self.kraddies.entry(&id) else {
            return Response::error(ErrorCode::NotFound, format!("No Kraddie named {name}"));
        };
        let owner = MemoryOwner::Kraddie(id.clone());
        let digests = self.kraddies.memory.book(&owner).map_or_default(|book| {
            book.digests()
                .iter()
                .rev()
                .map(|digest| DigestInfo {
                    id: krad_domain::MemoryRef::Digest {
                        owner: owner.clone(),
                        digest: digest.id,
                    }
                    .to_string(),
                    chat: digest.chat.title.clone(),
                    facts: digest.facts.clone(),
                })
                .collect()
        });
        Response::KraddieRecall(KraddieRecallInfo {
            kraddie: id.to_string(),
            keepsakes: self.kraddie_mind(&id).memory.lines().to_vec(),
            digests,
            reads: entry
                .kraddie
                .settings
                .reads
                .iter()
                .map(ToString::to_string)
                .collect(),
            automatic_digests: entry.kraddie.settings.digests.is_automatic(),
        })
    }

    fn share_from_bus(
        &mut self,
        reader: &str,
        from: &str,
        switch: Switch,
        cx: &mut Context<Self>,
    ) -> Response {
        let (reader, from) = match (self.kraddie_by_name(reader), self.kraddie_by_name(from)) {
            (Ok(reader), Ok(from)) => (reader, from),
            (Err(response), _) | (_, Err(response)) => return response,
        };
        if reader == from {
            return Response::error(ErrorCode::BadRequest, OWN_MEMORY);
        }
        self.set_memory_sharing(&reader, &from, switch, cx);
        let reads = self.kraddies.entry(&reader).map_or_default(|entry| {
            entry
                .kraddie
                .settings
                .reads
                .iter()
                .map(ToString::to_string)
                .collect()
        });
        Response::KraddieShared(KraddieSharingInfo {
            kraddie: reader.to_string(),
            reads,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{
        AgentPreset, DigestBook, DigestChat, DigestDraft, Kraddie, KraddieMemory, SessionId,
    };
    use std::time::SystemTime;

    fn id(text: &str) -> KraddieId {
        KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!())
    }

    fn setup() -> (MemoryLibrary, Vec<Kraddie>) {
        let a = Kraddie::new(
            "A".parse().unwrap_or_else(|_| unreachable!()),
            AgentPreset::ClaudeCode,
        );
        let b = Kraddie::new(
            "B".parse().unwrap_or_else(|_| unreachable!()),
            AgentPreset::ClaudeCode,
        );
        let mut memory = KraddieMemory::default();
        assert!(memory
            .add("B keeps the staging password rotation notes")
            .is_ok());
        let mut ordinary = DigestBook::default();
        ordinary.record(DigestDraft {
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
        });
        let mut library = MemoryLibrary::default();
        library.add_keepsakes(&id("b"), &memory);
        library.add_digests(&MemoryOwner::Ordinary, &ordinary);
        (library, vec![a, b])
    }

    #[test]
    fn a_chat_reads_only_the_kraddies_in_its_reach_and_never_edits_memory() {
        let (_, kraddies) = setup();
        let named = |name: &str| {
            kraddies
                .iter()
                .find(|kraddie| kraddie.name.matches(name))
                .map(Kraddie::id)
        };
        let a = memory_reach(&MemoryOwner::Kraddie(id("a")), &kraddies, &[]);
        let mentioned = memory_reach(&MemoryOwner::Kraddie(id("a")), &kraddies, &[id("b")]);
        let read_b = Request::KraddieRecall {
            name: "B".to_owned(),
            view: krad_agents::RecallView::default(),
        };
        let code = |request: &Request, reach: Option<&MemoryReach>| {
            memory_refusal(request, reach, named).map(|error| error.code)
        };
        assert_eq!(code(&read_b, Some(&a)), Some(ErrorCode::OutOfReach));
        let memory_b = Request::KraddieMemory {
            name: "B".to_owned(),
        };
        assert_eq!(code(&memory_b, Some(&a)), Some(ErrorCode::OutOfReach));
        assert_eq!(code(&read_b, Some(&mentioned)), None);
        let own = Request::KraddieMemory {
            name: "A".to_owned(),
        };
        assert_eq!(code(&own, Some(&a)), None);
        assert_eq!(code(&read_b, None), None);
        let edits = [
            Request::RememberKraddie {
                name: "A".to_owned(),
                text: "Deploys need a tag".to_owned(),
            },
            Request::ShareKraddieMemory {
                reader: "A".to_owned(),
                from: "B".to_owned(),
                switch: Switch::On,
            },
        ];
        for edit in &edits {
            assert_eq!(code(edit, Some(&mentioned)), Some(ErrorCode::Forbidden));
            assert_eq!(code(edit, None), None);
        }
    }

    #[test]
    fn the_bus_enforces_the_rules_on_every_memory_call() {
        let (library, kraddies) = setup();
        let a = memory_reach(&MemoryOwner::Kraddie(id("a")), &kraddies, &[]);
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
        let mentioned = memory_reach(&MemoryOwner::Kraddie(id("a")), &kraddies, &[id("b")]);
        assert!(matches!(
            memory_answer(&library, &mentioned, &read),
            Response::MemoryEntry(entry) if entry.text.contains("rotation")
        ));
        let bad = Request::MemoryRead {
            id: "nonsense".to_owned(),
        };
        assert!(matches!(
            memory_answer(&library, &a, &bad),
            Response::Error(error) if error.code == ErrorCode::BadRequest
        ));
        let missing = Request::MemoryRead {
            id: "chats/digest/42".to_owned(),
        };
        assert!(matches!(
            memory_answer(&library, &a, &missing),
            Response::Error(error) if error.code == ErrorCode::NotFound
        ));
    }
}
