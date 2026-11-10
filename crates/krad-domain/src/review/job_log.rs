mod redact;

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, BufRead};

pub(crate) use redact::redact_secrets;
use redact::SecretScanner;

pub const LOG_EXCERPT_LINES: usize = 120;
pub const MAX_LOG_LINE_BYTES: usize = 2048;
const CUT_MARK: &str = " […]";
const LINE_END: u8 = b'\n';
const ESCAPE: char = '\u{1b}';
const CSI_OPEN: char = '[';
const STRING_OPENERS: [char; 5] = [']', 'P', 'X', '^', '_'];
const INTERMEDIATE: std::ops::RangeInclusive<char> = '\u{20}'..='\u{2f}';
const BELL: char = '\u{7}';
const STRING_TERMINATOR: char = '\\';
const CARRIAGE_RETURN: char = '\r';
const TAB: char = '\t';
const CSI_FINAL: std::ops::RangeInclusive<char> = '\u{40}'..='\u{7e}';

#[must_use]
pub fn strip_ansi(text: &str) -> Cow<'_, str> {
    if !text
        .chars()
        .any(|character| character == ESCAPE || is_hidden_control(character))
    {
        return Cow::Borrowed(text);
    }
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != ESCAPE {
            if !is_hidden_control(character) {
                plain.push(character);
            }
            continue;
        }
        match chars.next() {
            Some(CSI_OPEN) => {
                for next in chars.by_ref() {
                    if CSI_FINAL.contains(&next) {
                        break;
                    }
                }
            }
            Some(opener) if STRING_OPENERS.contains(&opener) => {
                while let Some(next) = chars.next() {
                    if next == BELL {
                        break;
                    }
                    if next == ESCAPE && chars.peek() == Some(&STRING_TERMINATOR) {
                        chars.next();
                        break;
                    }
                }
            }
            Some(intermediate) if INTERMEDIATE.contains(&intermediate) => {
                while chars.next_if(|next| INTERMEDIATE.contains(next)).is_some() {}
                chars.next();
            }
            _ => {}
        }
    }
    Cow::Owned(plain)
}

fn is_hidden_control(character: char) -> bool {
    character.is_control() && character != TAB && character != CARRIAGE_RETURN
}

fn visible_text(text: &str) -> &str {
    let text = text.trim_end_matches(CARRIAGE_RETURN);
    text.rsplit(CARRIAGE_RETURN).next().unwrap_or(text)
}

/// # Errors
///
/// Returns the reader's error. A line longer than [`MAX_LOG_LINE_BYTES`] is
/// read to its end but only its start is kept, so one huge line cannot fill
/// memory.
pub fn read_log_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<usize> {
    line.clear();
    let mut read = 0;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(read);
        }
        let end = available.iter().position(|byte| *byte == LINE_END);
        let taken = end.map_or(available.len(), |at| at + 1);
        let room = (MAX_LOG_LINE_BYTES + 1).saturating_sub(line.len());
        line.extend_from_slice(&available[..taken.min(room)]);
        reader.consume(taken);
        read += taken;
        if end.is_some() {
            return Ok(read);
        }
    }
}

fn capped(line: Cow<'_, str>) -> String {
    if line.len() <= MAX_LOG_LINE_BYTES {
        return line.into_owned();
    }
    let mut end = MAX_LOG_LINE_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let kept = &line[..end];
    let kept = kept
        .rfind(char::is_whitespace)
        .map_or(kept, |space| &kept[..space]);
    format!("{kept}{CUT_MARK}")
}

fn clean_log_line(raw: &str, scanner: &mut SecretScanner) -> String {
    let plain = strip_ansi(raw);
    capped(scanner.redact(visible_text(&plain)))
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct StepLines {
    kept: VecDeque<String>,
    omitted: usize,
    scanner: SecretScanner,
}

impl StepLines {
    pub fn push(&mut self, raw: &str) {
        if self.kept.len() == LOG_EXCERPT_LINES {
            self.kept.pop_front();
            self.omitted += 1;
        }
        let line = clean_log_line(raw, &mut self.scanner);
        self.kept.push_back(line);
    }

    pub fn restart(&mut self) {
        self.kept.clear();
        self.omitted = 0;
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct JobLog {
    failing_step: Option<String>,
    lines: Vec<String>,
    omitted: usize,
}

impl JobLog {
    #[must_use]
    pub fn new(failing_step: Option<&str>, lines: StepLines) -> Self {
        Self {
            failing_step: failing_step
                .map(|step| clean_log_line(step, &mut SecretScanner::default()))
                .filter(|step| !step.trim().is_empty()),
            lines: lines.kept.into(),
            omitted: lines.omitted,
        }
    }

    #[must_use]
    pub fn failing_step(&self) -> Option<&str> {
        self.failing_step.as_deref()
    }

    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    #[must_use]
    pub const fn omitted(&self) -> usize {
        self.omitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORED_FIXTURE: &str = "\u{1b}[31;1merror[E0308]\u{1b}[0m: mismatched types\u{1b}[K";
    const OSC_FIXTURE: &str = "\u{1b}]0;cargo test\u{7}running 3 tests\u{1b}]8;;https://x\u{1b}\\";
    const PROGRESS_FIXTURE: &str = "Downloading 10%\rDownloading 50%\rDownloaded\r";

    #[test]
    fn colors_links_and_titles_leave_only_their_text() {
        assert_eq!(
            strip_ansi(COLORED_FIXTURE),
            "error[E0308]: mismatched types"
        );
        assert_eq!(strip_ansi(OSC_FIXTURE), "running 3 tests");
        assert!(matches!(strip_ansi("plain text"), Cow::Borrowed(_)));
    }

    #[test]
    fn every_escape_sequence_is_removed_not_only_colors_and_titles() {
        let charset = "\u{1b}(Bascii\u{1b})0";
        let keypad = "\u{1b}=\u{1b}7saved\u{1b}8\u{1b}>";
        let device = "\u{1b}P1$r0m\u{1b}\\dcs gone\u{1b}_apc\u{7}\u{1b}^pm\u{1b}\\";
        assert_eq!(strip_ansi(charset), "ascii");
        assert_eq!(strip_ansi(keypad), "saved");
        assert_eq!(strip_ansi(device), "dcs gone");
    }

    #[test]
    fn a_huge_line_is_cut_when_read_and_when_kept() -> io::Result<()> {
        let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123";
        let filler = "a".repeat(MAX_LOG_LINE_BYTES - 8);
        let huge = format!(
            "{filler} {secret} {}\nnext\n",
            "b".repeat(MAX_LOG_LINE_BYTES)
        );
        let mut reader = io::Cursor::new(huge.as_bytes());
        let mut line = Vec::new();
        assert_eq!(
            read_log_line(&mut reader, &mut line)?,
            huge.len() - "next\n".len()
        );
        assert_eq!(line.len(), MAX_LOG_LINE_BYTES + 1);
        let mut steps = StepLines::default();
        steps.push(&String::from_utf8_lossy(&line));
        let kept = &steps.kept[0];
        assert!(kept.len() <= MAX_LOG_LINE_BYTES + CUT_MARK.len());
        assert!(kept.ends_with(CUT_MARK));
        assert!(!kept.contains("ghp_"));
        assert_eq!(read_log_line(&mut reader, &mut line)?, "next\n".len());
        assert_eq!(line, b"next\n");
        assert_eq!(read_log_line(&mut reader, &mut line)?, 0);
        Ok(())
    }

    #[test]
    fn a_line_redrawn_with_carriage_returns_keeps_its_last_drawing() {
        let mut scanner = SecretScanner::default();
        assert_eq!(clean_log_line(PROGRESS_FIXTURE, &mut scanner), "Downloaded");
        assert_eq!(clean_log_line("\tindented", &mut scanner), "\tindented");
    }

    #[test]
    fn a_step_keeps_only_its_last_lines_and_counts_the_rest() {
        let mut lines = StepLines::default();
        let total = LOG_EXCERPT_LINES + 5;
        for line in 0..total {
            lines.push(&format!("line {line}"));
        }
        let log = JobLog::new(Some("test"), lines);
        assert_eq!(log.lines.len(), LOG_EXCERPT_LINES);
        assert_eq!(log.omitted, 5);
        assert_eq!(log.lines.first().map(String::as_str), Some("line 5"));
        assert_eq!(
            log.lines.last().map(String::as_str),
            Some(format!("line {}", total - 1).as_str())
        );
    }

    #[test]
    fn restarting_a_step_forgets_the_previous_one() {
        let mut lines = StepLines::default();
        lines.push("setup");
        lines.restart();
        lines.push("\u{1b}[1mcargo test\u{1b}[0m");
        let log = JobLog::new(Some(" "), lines);
        assert_eq!(log.lines, vec!["cargo test".to_owned()]);
        assert_eq!(log.failing_step, None);
    }
}
