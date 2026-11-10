use super::owner::MemoryOwner;
use crate::kraddie::{Kraddie, KraddieId};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryReach {
    reader: MemoryOwner,
    kraddies: BTreeSet<KraddieId>,
}

impl MemoryReach {
    #[must_use]
    pub fn allows(&self, owner: &MemoryOwner) -> bool {
        match owner {
            MemoryOwner::Ordinary => true,
            MemoryOwner::Kraddie(id) => self.kraddies.contains(id),
        }
    }

    #[must_use]
    pub const fn reader(&self) -> &MemoryOwner {
        &self.reader
    }

    pub fn kraddies(&self) -> impl Iterator<Item = &KraddieId> {
        self.kraddies.iter()
    }
}

#[must_use]
pub fn memory_reach<'a>(
    reader: &MemoryOwner,
    kraddies: impl IntoIterator<Item = &'a Kraddie>,
    mentioned: &[KraddieId],
) -> MemoryReach {
    let known: Vec<&Kraddie> = kraddies.into_iter().collect();
    let exists = |id: &KraddieId| known.iter().any(|kraddie| kraddie.id() == *id);
    let own: Vec<KraddieId> = match reader {
        MemoryOwner::Ordinary => Vec::new(),
        MemoryOwner::Kraddie(id) => known
            .iter()
            .find(|kraddie| kraddie.id() == *id)
            .map_or_default(|kraddie| {
                std::iter::once(id.clone())
                    .chain(kraddie.settings.reads.iter().cloned())
                    .collect()
            }),
    };
    let kraddies = own
        .into_iter()
        .chain(mentioned.iter().cloned())
        .filter(|id| exists(id))
        .collect();
    MemoryReach {
        reader: reader.clone(),
        kraddies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentPreset;

    fn kraddie(name: &str, reads: &[&str]) -> Kraddie {
        let mut kraddie = Kraddie::new(
            name.parse().unwrap_or_else(|_| unreachable!()),
            AgentPreset::ClaudeCode,
        );
        kraddie.settings.reads = reads.iter().map(|read| id(read)).collect();
        kraddie
    }

    fn id(text: &str) -> KraddieId {
        KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!())
    }

    fn of(name: &str) -> MemoryOwner {
        MemoryOwner::Kraddie(id(name))
    }

    fn sees(reach: &MemoryReach) -> [bool; 4] {
        [
            reach.allows(&MemoryOwner::Ordinary),
            reach.allows(&of("a")),
            reach.allows(&of("b")),
            reach.allows(&of("c")),
        ]
    }

    #[test]
    fn the_visibility_matrix_follows_ownership_sharing_and_mentions() {
        let kraddies = [
            kraddie("A", &["b"]),
            kraddie("B", &[]),
            kraddie("C", &["a"]),
        ];
        let ordinary = memory_reach(&MemoryOwner::Ordinary, &kraddies, &[]);
        assert_eq!(sees(&ordinary), [true, false, false, false]);
        let a = memory_reach(&of("a"), &kraddies, &[]);
        assert_eq!(sees(&a), [true, true, true, false], "A reads B");
        let b = memory_reach(&of("b"), &kraddies, &[]);
        assert_eq!(sees(&b), [true, false, true, false], "B does not read A");
        let c = memory_reach(&of("c"), &kraddies, &[]);
        assert_eq!(
            sees(&c),
            [true, true, false, true],
            "sharing is not transitive"
        );
    }

    #[test]
    fn a_mention_adds_the_kraddie_for_that_chat_only() {
        let kraddies = [kraddie("A", &[]), kraddie("B", &[])];
        let b_mentions_a = memory_reach(&of("b"), &kraddies, &[id("a")]);
        assert_eq!(sees(&b_mentions_a), [true, true, true, false]);
        let ordinary_mentions_b = memory_reach(&MemoryOwner::Ordinary, &kraddies, &[id("b")]);
        assert_eq!(sees(&ordinary_mentions_b), [true, false, true, false]);
        assert!(kraddies
            .iter()
            .all(|kraddie| kraddie.settings.reads.is_empty()));
    }

    #[test]
    fn unknown_kraddies_are_never_reached() {
        let kraddies = [kraddie("A", &["gone"])];
        let a = memory_reach(&of("a"), &kraddies, &[id("ghost")]);
        assert_eq!(a.kraddies().cloned().collect::<Vec<_>>(), [id("a")]);
        let deleted = memory_reach(&of("deleted"), &kraddies, &[]);
        assert_eq!(sees(&deleted), [true, false, false, false]);
    }
}
