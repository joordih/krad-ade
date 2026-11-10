use serde::{Deserialize, Serialize};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;
const LINE_END: u8 = b'\n';

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HunkFingerprint(u64);

impl HunkFingerprint {
    #[must_use]
    pub fn new<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let hash = parts.into_iter().fold(FNV_OFFSET, |hash, part| {
            part.iter()
                .chain(std::iter::once(&0))
                .fold(hash, |hash, byte| {
                    (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
                })
        });
        Self(hash)
    }

    #[must_use]
    pub fn occurrence(self, index: usize) -> Self {
        if index == 0 {
            return self;
        }
        let hash = self.0.to_le_bytes();
        let index = u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes();
        Self::new([hash.as_slice(), index.as_slice()])
    }
}

impl<'a> FromIterator<&'a str> for HunkFingerprint {
    fn from_iter<I: IntoIterator<Item = &'a str>>(lines: I) -> Self {
        let hash = lines
            .into_iter()
            .flat_map(|line| line.bytes().chain(std::iter::once(LINE_END)))
            .fold(FNV_OFFSET, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
            });
        Self(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_occurrence_keeps_its_print_and_later_ones_differ() {
        let print = HunkFingerprint::new([b"a.rs".as_slice(), b"+x".as_slice()]);
        assert_eq!(print.occurrence(0), print);
        assert_ne!(print.occurrence(1), print);
        assert_ne!(print.occurrence(1), print.occurrence(2));
        assert_eq!(print.occurrence(1), print.occurrence(1));
    }
}
