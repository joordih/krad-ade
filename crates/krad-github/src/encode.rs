const UNRESERVED_MARKS: &[u8] = b"-._~/";
const PERCENT: char = '%';
const UPPER_HEX: &[u8; 16] = b"0123456789ABCDEF";
const NIBBLE_BITS: u8 = 4;
const LOW_NIBBLE: u8 = 0x0f;

pub(crate) fn encode_path(path: &str) -> String {
    path.bytes().fold(String::new(), |mut encoded, byte| {
        if byte.is_ascii_alphanumeric() || UNRESERVED_MARKS.contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push(PERCENT);
            encoded.push(char::from(UPPER_HEX[usize::from(byte >> NIBBLE_BITS)]));
            encoded.push(char::from(UPPER_HEX[usize::from(byte & LOW_NIBBLE)]));
        }
        encoded
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_keeps_its_folders_and_escapes_the_rest() {
        assert_eq!(encode_path("docs/a b#1.md"), "docs/a%20b%231.md");
    }
}
