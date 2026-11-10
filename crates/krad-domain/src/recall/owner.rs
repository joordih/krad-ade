use crate::kraddie::KraddieId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const ORDINARY_LABEL: &str = "chats";
const KRADDIE_LABEL: &str = "kraddie";
const PART_SEPARATOR: char = '/';
const KEEPSAKE_LABEL: &str = "keepsake";
const DIGEST_LABEL: &str = "digest";

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOwner {
    Ordinary,
    Kraddie(KraddieId),
}

impl MemoryOwner {
    #[must_use]
    pub const fn kraddie(&self) -> Option<&KraddieId> {
        match self {
            Self::Ordinary => None,
            Self::Kraddie(id) => Some(id),
        }
    }
}

impl From<Option<KraddieId>> for MemoryOwner {
    fn from(kraddie: Option<KraddieId>) -> Self {
        kraddie.map_or(Self::Ordinary, Self::Kraddie)
    }
}

impl fmt::Display for MemoryOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ordinary => f.write_str(ORDINARY_LABEL),
            Self::Kraddie(id) => write!(f, "{KRADDIE_LABEL}{PART_SEPARATOR}{id}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DigestId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemoryRef {
    Keepsake {
        kraddie: KraddieId,
        line: usize,
    },
    Digest {
        owner: MemoryOwner,
        digest: DigestId,
    },
}

impl MemoryRef {
    #[must_use]
    pub fn owner(&self) -> MemoryOwner {
        match self {
            Self::Keepsake { kraddie, .. } => MemoryOwner::Kraddie(kraddie.clone()),
            Self::Digest { owner, .. } => owner.clone(),
        }
    }
}

impl fmt::Display for MemoryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keepsake { kraddie, line } => write!(
                f,
                "{KRADDIE_LABEL}{PART_SEPARATOR}{kraddie}{PART_SEPARATOR}{KEEPSAKE_LABEL}{PART_SEPARATOR}{}",
                line + 1
            ),
            Self::Digest { owner, digest } => write!(
                f,
                "{owner}{PART_SEPARATOR}{DIGEST_LABEL}{PART_SEPARATOR}{}",
                digest.0
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownMemoryRef;

impl fmt::Display for UnknownMemoryRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "Not a memory id: use one memory_search returned, such as chats/digest/3 or kraddie/<id>/keepsake/1",
        )
    }
}

impl std::error::Error for UnknownMemoryRef {}

impl FromStr for MemoryRef {
    type Err = UnknownMemoryRef;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = text.trim().split(PART_SEPARATOR).collect();
        let number = |part: &str| part.parse::<u64>().map_err(|_| UnknownMemoryRef);
        match parts.as_slice() {
            [ORDINARY_LABEL, DIGEST_LABEL, digest] => Ok(Self::Digest {
                owner: MemoryOwner::Ordinary,
                digest: DigestId(number(digest)?),
            }),
            [KRADDIE_LABEL, kraddie, kind, at] => {
                let kraddie =
                    KraddieId::try_from((*kraddie).to_owned()).map_err(|_| UnknownMemoryRef)?;
                match *kind {
                    KEEPSAKE_LABEL => {
                        let line = usize::try_from(number(at)?).map_err(|_| UnknownMemoryRef)?;
                        let line = line.checked_sub(1).ok_or(UnknownMemoryRef)?;
                        Ok(Self::Keepsake { kraddie, line })
                    }
                    DIGEST_LABEL => Ok(Self::Digest {
                        owner: MemoryOwner::Kraddie(kraddie),
                        digest: DigestId(number(at)?),
                    }),
                    _ => Err(UnknownMemoryRef),
                }
            }
            _ => Err(UnknownMemoryRef),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> KraddieId {
        KraddieId::try_from(text.to_owned()).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn references_print_and_parse_back() {
        let references = [
            MemoryRef::Keepsake {
                kraddie: id("night-owl"),
                line: 0,
            },
            MemoryRef::Digest {
                owner: MemoryOwner::Kraddie(id("night-owl")),
                digest: DigestId(7),
            },
            MemoryRef::Digest {
                owner: MemoryOwner::Ordinary,
                digest: DigestId(3),
            },
        ];
        let printed: Vec<String> = references.iter().map(ToString::to_string).collect();
        assert_eq!(
            printed,
            [
                "kraddie/night-owl/keepsake/1",
                "kraddie/night-owl/digest/7",
                "chats/digest/3"
            ]
        );
        for (reference, text) in references.iter().zip(&printed) {
            assert_eq!(text.parse::<MemoryRef>().as_ref(), Ok(reference));
        }
    }

    #[test]
    fn malformed_references_are_refused() {
        for bad in [
            "",
            "chats/digest/x",
            "kraddie/Upper/digest/1",
            "kraddie/a/keepsake/0",
            "kraddie/a/other/1",
            "chats/keepsake/1",
        ] {
            assert_eq!(bad.parse::<MemoryRef>(), Err(UnknownMemoryRef), "{bad}");
        }
    }
}
