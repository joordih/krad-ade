use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::BusToken;

#[must_use]
pub fn new_token() -> BusToken {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or_default(|elapsed| elapsed.as_nanos());
    let halves = [RandomState::new(), RandomState::new()].map(|state| {
        let mut hasher = state.build_hasher();
        std::process::id().hash(&mut hasher);
        nanos.hash(&mut hasher);
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
