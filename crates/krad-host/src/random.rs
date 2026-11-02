#[path = "random/fallback.rs"]
mod fallback;
#[cfg(unix)]
#[path = "random/unix.rs"]
mod unix;

use std::fmt::Write as _;

pub(crate) const TOKEN_BYTES: usize = 32;
const INSTALL_ID_BYTES: usize = 16;

#[cfg(unix)]
fn fill(buffer: &mut [u8]) {
    if let Err(error) = unix::fill(buffer) {
        tracing::warn!(%error, "the system random source failed; using the hasher keys");
        fallback::fill(buffer);
    }
}

#[cfg(not(unix))]
fn fill(buffer: &mut [u8]) {
    fallback::fill(buffer);
}

pub(crate) fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0; bytes];
    fill(&mut buffer);
    buffer
        .iter()
        .fold(String::with_capacity(bytes * 2), |mut text, byte| {
            write!(text, "{byte:02x}").ok();
            text
        })
}

#[must_use]
pub fn new_install_id() -> Option<krad_domain::InstallId> {
    random_hex(INSTALL_ID_BYTES).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_hex_has_the_asked_length_and_differs() {
        let first = random_hex(TOKEN_BYTES);
        assert_eq!(first.len(), TOKEN_BYTES * 2);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(random_hex(TOKEN_BYTES), first);
    }

    #[test]
    fn a_new_install_id_is_a_valid_random_id() {
        let first = new_install_id();
        assert!(first.is_some());
        assert_ne!(new_install_id(), first);
    }

    #[test]
    fn the_fallback_fills_every_byte_position() {
        let mut buffer = [0_u8; 19];
        fallback::fill(&mut buffer);
        assert!(buffer.iter().any(|byte| *byte != 0));
    }
}
