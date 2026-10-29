use crate::encode::base64_url;
use std::hash::{BuildHasher, RandomState};

const SECRET_WORDS: u64 = 4;
const BLOCK_BYTES: usize = 64;
const WORD_BYTES: usize = 4;
const LENGTH_BYTES: usize = 8;
const PADDING_MARK: u8 = 0x80;
const SCHEDULE_WORDS: usize = 64;
const BITS_PER_BYTE: u64 = 8;
const INITIAL_STATE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];
const ROUND_CONSTANTS: [u32; SCHEDULE_WORDS] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

pub(crate) struct Pkce {
    pub(crate) verifier: String,
    pub(crate) challenge: String,
}

impl Pkce {
    pub(crate) fn new() -> Self {
        Self::from_verifier(random_secret())
    }

    fn from_verifier(verifier: String) -> Self {
        let challenge = base64_url(&sha256(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

pub(crate) fn random_secret() -> String {
    let keyed = RandomState::new();
    let bytes: Vec<u8> = (0..SECRET_WORDS)
        .flat_map(|word| keyed.hash_one(word).to_le_bytes())
        .collect();
    base64_url(&bytes)
}

fn sha256(message: &[u8]) -> [u8; 32] {
    let mut padded = message.to_vec();
    padded.push(PADDING_MARK);
    while padded.len() % BLOCK_BYTES != BLOCK_BYTES - LENGTH_BYTES {
        padded.push(0);
    }
    let bit_length = (message.len() as u64).wrapping_mul(BITS_PER_BYTE);
    padded.extend_from_slice(&bit_length.to_be_bytes());
    let state = padded
        .as_chunks::<BLOCK_BYTES>()
        .0
        .iter()
        .fold(INITIAL_STATE, compress);
    let mut digest = [0u8; 32];
    for (bytes, word) in digest.as_chunks_mut::<WORD_BYTES>().0.iter_mut().zip(state) {
        *bytes = word.to_be_bytes();
    }
    digest
}

#[expect(
    clippy::many_single_char_names,
    reason = "FIPS 180-4 names the working variables a to h"
)]
fn compress(state: [u32; 8], block: &[u8; BLOCK_BYTES]) -> [u32; 8] {
    let mut schedule = [0u32; SCHEDULE_WORDS];
    for (word, bytes) in schedule.iter_mut().zip(block.as_chunks::<WORD_BYTES>().0) {
        *word = u32::from_be_bytes(*bytes);
    }
    for index in 16..SCHEDULE_WORDS {
        let early = schedule[index - 15];
        let late = schedule[index - 2];
        let s0 = early.rotate_right(7) ^ early.rotate_right(18) ^ (early >> 3);
        let s1 = late.rotate_right(17) ^ late.rotate_right(19) ^ (late >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }
    let mut work = state;
    for (constant, word) in ROUND_CONSTANTS.iter().zip(schedule) {
        let [a, b, c, d, e, f, g, h] = work;
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(*constant)
            .wrapping_add(word);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);
        work = [
            temp1.wrapping_add(temp2),
            a,
            b,
            c,
            d.wrapping_add(temp1),
            e,
            f,
            g,
        ];
    }
    let mut next = state;
    for (word, worked) in next.iter_mut().zip(work) {
        *word = word.wrapping_add(worked);
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .flat_map(|byte| [byte >> 4, byte & 0x0f])
            .map(|nibble| char::from_digit(u32::from(nibble), 16).unwrap_or('?'))
            .collect()
    }

    #[test]
    fn sha256_matches_the_fips_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn the_challenge_matches_rfc_7636_appendix_b() {
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned());
        assert_eq!(
            pkce.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn secrets_are_long_enough_and_differ() {
        let first = random_secret();
        assert_eq!(first.len(), 43);
        assert_ne!(first, random_secret());
    }
}
