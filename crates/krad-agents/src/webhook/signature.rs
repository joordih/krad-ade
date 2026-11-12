use krad_domain::WebhookSecret;
use sha2::{Digest, Sha256};

const BLOCK_LEN: usize = 64;
const INNER_PAD: u8 = 0x36;
const OUTER_PAD: u8 = 0x5c;
const SIGNATURE_PREFIX: &str = "sha256=";
const SIGNED_SEPARATOR: &[u8] = b".";
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
const NIBBLE_BITS: u8 = 4;
const LOW_NIBBLE: u8 = 0x0f;

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0_u8; BLOCK_LEN];
    if key.len() > BLOCK_LEN {
        let hashed = Sha256::digest(key);
        block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner_key = block.map(|byte| byte ^ INNER_PAD);
    let outer_key = block.map(|byte| byte ^ OUTER_PAD);
    let inner = Sha256::new()
        .chain_update(inner_key)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(outer_key)
        .chain_update(inner)
        .finalize()
        .into()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte >> NIBBLE_BITS, byte & LOW_NIBBLE])
        .map(|nibble| char::from(HEX_DIGITS[usize::from(nibble)]))
        .collect()
}

pub(crate) fn signature_header(secret: &WebhookSecret, sent_at: &str, body: &[u8]) -> String {
    let signed = [sent_at.as_bytes(), SIGNED_SEPARATOR, body].concat();
    let digest = hmac_sha256(secret.expose().as_bytes(), &signed);
    format!("{SIGNATURE_PREFIX}{}", hex(&digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_the_rfc_4231_vectors() {
        assert_eq!(
            hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex(&hmac_sha256(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn the_header_signs_the_timestamp_a_dot_and_the_body() {
        let secret = WebhookSecret::parse("Jefe").unwrap_or_else(|| unreachable!("a secret"));
        let sent_at = "2026-11-10T09:15:00.120Z";
        let body = b"{\"api\":1}";
        let expected = hex(&hmac_sha256(
            b"Jefe",
            b"2026-11-10T09:15:00.120Z.{\"api\":1}",
        ));
        assert_eq!(
            signature_header(&secret, sent_at, body),
            format!("sha256={expected}")
        );
        assert_ne!(
            signature_header(&secret, "2026-11-10T09:20:00.000Z", body),
            signature_header(&secret, sent_at, body)
        );
    }
}
