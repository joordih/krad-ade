use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::time::Instant;

use crate::protocol::BusToken;

#[must_use]
pub fn new_token() -> BusToken {
    let now = Instant::now();
    let halves = [RandomState::new(), RandomState::new()].map(|state| {
        let mut hasher = state.build_hasher();
        std::process::id().hash(&mut hasher);
        now.hash(&mut hasher);
        hasher.finish()
    });
    BusToken::from(format!("{:016x}{:016x}", halves[0], halves[1]))
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
}
