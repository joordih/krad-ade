use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::time::Instant;

use crate::protocol::BusToken;

#[must_use]
pub fn new_token() -> BusToken {
    BusToken::from(random_hex())
}

#[must_use]
pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let difference = left
        .iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            std::hint::black_box(difference | (left ^ right))
        });
    difference == 0
}

pub(crate) fn random_hex() -> String {
    let now = Instant::now();
    let halves = [RandomState::new(), RandomState::new()].map(|state| {
        let mut hasher = state.build_hasher();
        std::process::id().hash(&mut hasher);
        now.hash(&mut hasher);
        hasher.finish()
    });
    format!("{:016x}{:016x}", halves[0], halves[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN_LEN: usize = 32;

    #[test]
    fn tokens_are_distinct_hex_strings() {
        let (first, second) = (new_token(), new_token());
        assert_ne!(first, second);
        for token in [first, second] {
            assert_eq!(token.as_str().len(), TOKEN_LEN);
            assert!(token.as_str().chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn tokens_compare_by_every_byte_and_by_length() {
        assert!(constant_time_eq(b"a1b2c3", b"a1b2c3"));
        assert!(constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"a1b2c3", b"a1b2c4"));
        assert!(!constant_time_eq(b"b1b2c3", b"a1b2c3"));
        assert!(!constant_time_eq(b"a1b2c3", b"a1b2c"));
        let token = new_token();
        assert_eq!(token.clone(), token);
        assert_ne!(BusToken::from("secret"), BusToken::from("secreT"));
    }
}
