use super::digest::DigestBook;
use super::owner::{MemoryOwner, MemoryRef};
use super::reach::MemoryReach;
use crate::chat_search::tokens;
use crate::kraddie::{KraddieId, KraddieMemory};
use std::collections::BTreeSet;
use std::fmt;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryKind {
    Keepsake,
    Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryItem {
    pub reference: MemoryRef,
    pub kind: MemoryKind,
    pub title: Option<String>,
    pub text: String,
    pub written: Option<SystemTime>,
}

impl MemoryItem {
    #[must_use]
    pub fn owner(&self) -> MemoryOwner {
        self.reference.owner()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemorySize {
    pub keepsakes: usize,
    pub digests: usize,
}

impl MemorySize {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.keepsakes == 0 && self.digests == 0
    }
}

impl fmt::Display for MemorySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plural = |count: usize| if count == 1 { SINGULAR } else { PLURAL };
        write!(
            f,
            "{} keepsake{} · {} chat digest{}",
            self.keepsakes,
            plural(self.keepsakes),
            self.digests,
            plural(self.digests)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryAccessError {
    NotFound,
    NotShared,
}

impl fmt::Display for MemoryAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "No memory with that id",
            Self::NotShared => "That memory is not shared with this chat",
        })
    }
}

impl std::error::Error for MemoryAccessError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryHit {
    pub reference: MemoryRef,
    pub title: Option<String>,
    pub excerpt: String,
    pub score: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryLibrary {
    items: Vec<MemoryItem>,
}

impl MemoryLibrary {
    pub fn add_keepsakes(&mut self, kraddie: &KraddieId, memory: &KraddieMemory) {
        self.items.extend(
            memory
                .lines()
                .iter()
                .enumerate()
                .map(|(line, text)| MemoryItem {
                    reference: MemoryRef::Keepsake {
                        kraddie: kraddie.clone(),
                        line,
                    },
                    kind: MemoryKind::Keepsake,
                    title: None,
                    text: text.clone(),
                    written: None,
                }),
        );
    }

    pub fn add_digests(&mut self, owner: &MemoryOwner, book: &DigestBook) {
        self.items
            .extend(book.digests().iter().map(|digest| MemoryItem {
                reference: MemoryRef::Digest {
                    owner: owner.clone(),
                    digest: digest.id,
                },
                kind: MemoryKind::Digest,
                title: Some(digest.chat.title.clone()),
                text: digest.facts.join(FACT_JOIN),
                written: Some(digest.written),
            }));
    }

    pub fn visible<'a>(&'a self, reach: &'a MemoryReach) -> impl Iterator<Item = &'a MemoryItem> {
        self.items.iter().filter(|item| reach.allows(&item.owner()))
    }

    pub fn of_owner<'a>(&'a self, owner: &'a MemoryOwner) -> impl Iterator<Item = &'a MemoryItem> {
        self.items.iter().filter(move |item| item.owner() == *owner)
    }

    #[must_use]
    pub fn size_of(&self, owner: &MemoryOwner) -> MemorySize {
        self.of_owner(owner)
            .fold(MemorySize::default(), |mut size, item| {
                match item.kind {
                    MemoryKind::Keepsake => size.keepsakes += 1,
                    MemoryKind::Digest => size.digests += 1,
                }
                size
            })
    }

    /// # Errors
    /// Fails when no item has the reference or the reach does not cover its owner.
    pub fn read(
        &self,
        reach: &MemoryReach,
        reference: &MemoryRef,
    ) -> Result<&MemoryItem, MemoryAccessError> {
        if !reach.allows(&reference.owner()) {
            return Err(MemoryAccessError::NotShared);
        }
        self.items
            .iter()
            .find(|item| item.reference == *reference)
            .ok_or(MemoryAccessError::NotFound)
    }

    #[must_use]
    pub fn search(&self, reach: &MemoryReach, query: &str, limit: usize) -> Vec<MemoryHit> {
        let terms = query_terms(query);
        let mut hits: Vec<MemoryHit> = self
            .visible(reach)
            .filter_map(|item| {
                let score = relevance(item, &terms);
                (score > 0).then(|| MemoryHit {
                    reference: item.reference.clone(),
                    title: item.title.clone(),
                    excerpt: excerpt(&item.text),
                    score,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.reference.cmp(&b.reference))
        });
        hits.truncate(limit);
        hits
    }
}

const FACT_JOIN: &str = "; ";
const SINGULAR: &str = "";
const PLURAL: &str = "s";
const EXCERPT_CHARS: usize = 240;
const ELLIPSIS: char = '…';

pub(super) fn query_terms(query: &str) -> BTreeSet<String> {
    tokens(query).map(|token| token.term).collect()
}

pub(super) fn relevance(item: &MemoryItem, terms: &BTreeSet<String>) -> u32 {
    let in_text = tokens(&item.text).filter(|token| terms.contains(&token.term));
    let in_title = item
        .title
        .iter()
        .flat_map(|title| tokens(title))
        .filter(|token| terms.contains(&token.term));
    let count = in_text.count() + in_title.count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let mut cut: String = chars.by_ref().take(EXCERPT_CHARS).collect();
    if chars.next().is_some() {
        cut.push(ELLIPSIS);
    }
    cut
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::digest::{DigestChat, DigestDraft};
    use super::super::reach::memory_reach;
    use super::*;
    use crate::agent::AgentPreset;
    use crate::ids::{EnvironmentName, SessionId, TerminalId};
    use crate::kraddie::Kraddie;
    use std::time::Duration;

    pub(in crate::recall) fn id(text: &str) -> KraddieId {
        KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!())
    }

    pub(in crate::recall) fn book(facts: &[(&str, &[&str], u64)]) -> DigestBook {
        let mut book = DigestBook::default();
        for (terminal, (title, facts, seconds)) in (1u64..).zip(facts) {
            book.record(DigestDraft {
                chat: DigestChat {
                    environment: EnvironmentName::new("main".to_owned()),
                    session: SessionId(1),
                    terminal: TerminalId(terminal),
                    title: (*title).to_owned(),
                    folder: None,
                    resume: None,
                },
                facts: facts.iter().map(|fact| (*fact).to_owned()).collect(),
                covered: 2,
                written: SystemTime::UNIX_EPOCH + Duration::from_secs(*seconds),
            });
        }
        book
    }

    pub(in crate::recall) fn library() -> (MemoryLibrary, Vec<Kraddie>) {
        let mut a = Kraddie::new(
            "A".parse().unwrap_or_else(|_| unreachable!()),
            AgentPreset::ClaudeCode,
        );
        a.settings.reads = vec![id("b")];
        let b = Kraddie::new(
            "B".parse().unwrap_or_else(|_| unreachable!()),
            AgentPreset::Codex,
        );
        let mut keepsakes = KraddieMemory::default();
        assert!(keepsakes.add("A deploys with the release script").is_ok());
        let mut library = MemoryLibrary::default();
        library.add_keepsakes(&id("a"), &keepsakes);
        library.add_digests(
            &MemoryOwner::Kraddie(id("a")),
            &book(&[("Release", &["Release notes live in CHANGES.md"], 30)]),
        );
        library.add_digests(
            &MemoryOwner::Kraddie(id("b")),
            &book(&[("Database", &["The database is Postgres 17"], 20)]),
        );
        library.add_digests(
            &MemoryOwner::Ordinary,
            &book(&[("Login", &["Login uses the Postgres session table"], 10)]),
        );
        (library, vec![a, b])
    }

    #[test]
    fn search_only_returns_what_the_chat_may_see() {
        let (library, kraddies) = library();
        let ordinary = memory_reach(&MemoryOwner::Ordinary, &kraddies, &[]);
        let hits = library.search(&ordinary, "postgres", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].reference.to_string(), "chats/digest/1");
        let a = memory_reach(&MemoryOwner::Kraddie(id("a")), &kraddies, &[]);
        assert_eq!(library.search(&a, "postgres", 10).len(), 2);
        let b = memory_reach(&MemoryOwner::Kraddie(id("b")), &kraddies, &[]);
        assert_eq!(library.search(&b, "release", 10).len(), 0);
        assert_eq!(library.search(&a, "release", 1).len(), 1);
    }

    #[test]
    fn reading_refuses_memory_the_rules_hide() {
        let (library, kraddies) = library();
        let b = memory_reach(&MemoryOwner::Kraddie(id("b")), &kraddies, &[]);
        let keepsake: MemoryRef = "kraddie/a/keepsake/1"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            library.read(&b, &keepsake),
            Err(MemoryAccessError::NotShared)
        );
        let a = memory_reach(&MemoryOwner::Kraddie(id("a")), &kraddies, &[]);
        assert_eq!(
            library.read(&a, &keepsake).map(|item| item.text.as_str()),
            Ok("A deploys with the release script")
        );
        let missing: MemoryRef = "chats/digest/99".parse().unwrap_or_else(|_| unreachable!());
        assert_eq!(library.read(&b, &missing), Err(MemoryAccessError::NotFound));
        let mentioned = memory_reach(&MemoryOwner::Kraddie(id("b")), &kraddies, &[id("a")]);
        assert!(library.read(&mentioned, &keepsake).is_ok());
    }

    #[test]
    fn sizes_count_keepsakes_and_digests_per_owner() {
        let (library, _) = library();
        let size = library.size_of(&MemoryOwner::Kraddie(id("a")));
        assert_eq!(
            size,
            MemorySize {
                keepsakes: 1,
                digests: 1
            }
        );
        assert_eq!(size.to_string(), "1 keepsake · 1 chat digest");
        assert!(library.size_of(&MemoryOwner::Kraddie(id("zz"))).is_empty());
    }
}
