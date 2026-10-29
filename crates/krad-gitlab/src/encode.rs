const BASE64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const BASE64_GROUP: usize = 3;
const SEXTET_MASK: u32 = 0x3f;
const UNRESERVED_MARKS: &[u8] = b"-._~";
const PERCENT: u8 = b'%';
const PLUS: u8 = b'+';
const SPACE: u8 = b' ';
const HEX_RADIX: u32 = 16;
const UPPER_HEX: &[u8; 16] = b"0123456789ABCDEF";
const NIBBLE_BITS: u8 = 4;
const LOW_NIBBLE: u8 = 0x0f;

pub(crate) fn percent_encode(text: &str) -> String {
    text.bytes().fold(String::new(), |mut encoded, byte| {
        if byte.is_ascii_alphanumeric() || UNRESERVED_MARKS.contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push(char::from(PERCENT));
            encoded.push(char::from(UPPER_HEX[usize::from(byte >> NIBBLE_BITS)]));
            encoded.push(char::from(UPPER_HEX[usize::from(byte & LOW_NIBBLE)]));
        }
        encoded
    })
}

pub(crate) fn form_decode(text: &str) -> Option<String> {
    let mut bytes = text.bytes();
    let mut decoded = Vec::with_capacity(text.len());
    while let Some(byte) = bytes.next() {
        decoded.push(match byte {
            PERCENT => {
                let high = char::from(bytes.next()?).to_digit(HEX_RADIX)?;
                let low = char::from(bytes.next()?).to_digit(HEX_RADIX)?;
                u8::try_from(high * HEX_RADIX + low).ok()?
            }
            PLUS => SPACE,
            byte => byte,
        });
    }
    String::from_utf8(decoded).ok()
}

pub(crate) fn base64_url(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(BASE64_GROUP) * 4);
    for group in bytes.chunks(BASE64_GROUP) {
        let word = group.iter().enumerate().fold(0u32, |word, (index, byte)| {
            word | u32::from(*byte) << (16 - 8 * index)
        });
        let sextets = group.len() + 1;
        for index in 0..sextets {
            let sextet = (word >> (18 - 6 * index)) & SEXTET_MASK;
            encoded.push(char::from(BASE64_URL[sextet as usize]));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_path_becomes_one_segment() {
        assert_eq!(percent_encode("group/sub project"), "group%2Fsub%20project");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn form_values_decode_escapes_and_plus() {
        assert_eq!(form_decode("a%2Fb+c").as_deref(), Some("a/b c"));
        assert_eq!(form_decode("bad%2"), None);
    }

    #[test]
    fn base64_url_matches_rfc_4648_vectors() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
    }
}
