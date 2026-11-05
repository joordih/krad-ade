use std::borrow::Cow;

const CONTINUATION_MASK: u8 = 0b1100_0000;
const CONTINUATION_BITS: u8 = 0b1000_0000;

#[derive(Debug)]
pub(super) struct Output {
    kept: Vec<u8>,
    limit: usize,
    truncated: bool,
}

impl Output {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            kept: Vec::new(),
            limit,
            truncated: false,
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) {
        self.kept.extend_from_slice(bytes);
        let over = self.kept.len().saturating_sub(self.limit);
        if over == 0 {
            return;
        }
        let cut = self
            .kept
            .iter()
            .skip(over)
            .position(|byte| !is_continuation(*byte))
            .map_or(self.kept.len(), |at| over + at);
        self.kept.drain(..cut);
        self.truncated = true;
    }

    pub(super) const fn truncated(&self) -> bool {
        self.truncated
    }

    pub(super) fn text(&self) -> String {
        decoded(&self.kept).into_owned()
    }
}

const fn is_continuation(byte: u8) -> bool {
    byte & CONTINUATION_MASK == CONTINUATION_BITS
}

fn decoded(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(error) if error.error_len().is_none() => {
            String::from_utf8_lossy(bytes.get(..error.valid_up_to()).unwrap_or_default())
        }
        Err(_) => String::from_utf8_lossy(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_over_its_limit_keeps_the_end_at_a_char_boundary() {
        let mut output = Output::new(2);
        output.push("aéb".as_bytes());
        assert!(output.truncated());
        assert_eq!(output.text(), "b");
        let mut roomy = Output::new(8);
        roomy.push("aéb".as_bytes());
        assert!(!roomy.truncated());
        assert_eq!(roomy.text(), "aéb");
    }

    #[test]
    fn a_char_split_across_chunks_waits_for_its_end() {
        let mut output = Output::new(16);
        let bytes = "é".as_bytes();
        output.push(&bytes[..1]);
        assert_eq!(output.text(), "");
        output.push(&bytes[1..]);
        assert_eq!(output.text(), "é");
    }
}
