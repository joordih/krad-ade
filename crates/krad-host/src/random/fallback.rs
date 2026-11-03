use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

const WORD_BYTES: usize = 8;

pub(super) fn fill(buffer: &mut [u8]) {
    for (index, chunk) in buffer.chunks_mut(WORD_BYTES).enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(index);
        let word = hasher.finish().to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
}
