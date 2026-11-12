use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct ProtocolVersion(pub u32);

impl ProtocolVersion {
    pub const CURRENT: Self = Self(1);
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct RequestId(pub u64);

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct Seq(pub u64);

impl Seq {
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seq_starts_at_zero_and_rises_by_one() {
        let first = Seq::default().next();
        assert_eq!(first, Seq(1));
        assert!(first.next() > first);
    }
}
