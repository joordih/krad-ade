const BOOTSTRAP: &str = r#"exec /bin/sh -c 'eval "$(printf "$1")"' sh"#;
const SOURCE_BOOTSTRAP: &str = r#"exec /bin/sh -c '. "$(printf "$1")"' sh"#;
const SINGLE_QUOTE: char = '\'';
const UNSAFE: &[u8] = b"'\\%!";
const PRINTABLE_FIRST: u8 = 0x20;
const PRINTABLE_LAST: u8 = 0x7e;
const ESCAPE: char = '\\';
const OCTAL_SHIFTS: [u8; 3] = [6, 3, 0];
const OCTAL_DIGIT: u8 = 0o7;
pub(crate) const INLINE_PAYLOAD_MAX_BYTES: usize = 96 * 1024;

fn is_literal(byte: u8) -> bool {
    (PRINTABLE_FIRST..=PRINTABLE_LAST).contains(&byte) && !UNSAFE.contains(&byte)
}

pub(crate) fn encode(script: &str) -> String {
    let mut encoded = String::with_capacity(script.len());
    for byte in script.bytes() {
        if is_literal(byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push(ESCAPE);
            for shift in OCTAL_SHIFTS {
                encoded.push(char::from(b'0' + ((byte >> shift) & OCTAL_DIGIT)));
            }
        }
    }
    encoded
}

pub(crate) fn inline(script: &str) -> Option<String> {
    let encoded = encode(script);
    (encoded.len() <= INLINE_PAYLOAD_MAX_BYTES)
        .then(|| format!("{BOOTSTRAP} {SINGLE_QUOTE}{encoded}{SINGLE_QUOTE}"))
}

pub(crate) fn sourcing(script_file: &str) -> String {
    format!(
        "{SOURCE_BOOTSTRAP} {SINGLE_QUOTE}{}{SINGLE_QUOTE}",
        encode(script_file)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn run_through(shell: &str, word: &str) -> std::io::Result<String> {
        let output = std::process::Command::new(shell)
            .arg("-c")
            .arg(word)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[test]
    fn the_sent_word_holds_only_characters_every_shell_reads_literally() {
        let script = "printf '%s\\n' \"it's\" 100% ! `x` é\n";
        let sent = inline(script).unwrap_or_default();
        let payload = sent.split_once(" sh ").map_or("", |(_, payload)| payload);
        assert_eq!(payload.matches(SINGLE_QUOTE).count(), 2);
        assert!(!payload.contains('\n'));
        assert!(!payload.contains('!'));
        assert!(!payload.contains('%'));
    }

    #[cfg(unix)]
    #[test]
    fn posix_shells_run_the_decoded_script_exactly() -> std::io::Result<()> {
        let script = "x=\"it's 100% \\\\ ok\"; printf '%s|' \"$x\" 'line\nbreak' 'é'";
        let word = inline(script).unwrap_or_default();
        let expected = "it's 100% \\ ok|line\nbreak|é|";
        for shell in [
            "/bin/sh",
            "/bin/dash",
            "/bin/bash",
            "/bin/zsh",
            "/bin/csh",
            "/bin/tcsh",
        ] {
            if std::path::Path::new(shell).exists() {
                assert_eq!(run_through(shell, &word)?, expected, "{shell}");
            }
        }
        Ok(())
    }

    #[test]
    fn huge_scripts_do_not_go_inline() {
        assert!(inline(&"x".repeat(INLINE_PAYLOAD_MAX_BYTES + 1)).is_none());
    }
}
