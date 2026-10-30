use std::fmt::Write as _;
use std::path::Path;

use super::Ran;

const DEFAULT_LIMIT: usize = 2000;
const MAX_LINE_CHARS: usize = 2000;
const BINARY_PROBE_BYTES: usize = 8192;
const FIRST_LINE: usize = 1;
const LINE_CUT_MARK: &str = "… [line cut]";
const EMPTY_TEXT: &str = "(empty file)";
const DIRECTORY_TEXT: &str = "is a folder; use find to list it";
const BINARY_TEXT: &str = "looks like a binary file and was not read";
const PAST_END_TEXT: &str = "has fewer lines than the offset";
const NUL: u8 = 0;

fn cut(line: &str) -> String {
    match line.char_indices().nth(MAX_LINE_CHARS) {
        Some((end, _)) => format!("{}{LINE_CUT_MARK}", &line[..end]),
        None => line.to_owned(),
    }
}

pub(super) fn read(path: &Path, shown: &str, offset: Option<usize>, limit: Option<usize>) -> Ran {
    if path.is_dir() {
        return Ran::failed(format!("{shown} {DIRECTORY_TEXT}"));
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return Ran::failed(format!("{shown}: {error}")),
    };
    if bytes
        .iter()
        .take(BINARY_PROBE_BYTES)
        .any(|byte| *byte == NUL)
    {
        return Ran::failed(format!("{shown} {BINARY_TEXT}"));
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Ran::done(EMPTY_TEXT.to_owned());
    }
    let first = offset.unwrap_or(FIRST_LINE).max(FIRST_LINE);
    if first > lines.len() {
        return Ran::failed(format!("{shown} {PAST_END_TEXT} ({} lines)", lines.len()));
    }
    let limit = limit
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(FIRST_LINE, DEFAULT_LIMIT);
    let shown_lines = &lines[first - FIRST_LINE..lines.len().min(first - FIRST_LINE + limit)];
    let mut content = String::new();
    for (at, line) in shown_lines.iter().enumerate() {
        let number = first + at;
        writeln!(content, "{number:>6}\t{}", cut(line)).unwrap_or_default();
    }
    let next = first + shown_lines.len();
    if next <= lines.len() {
        writeln!(
            content,
            "[{} of {} lines shown; continue with offset {next}]",
            next - first,
            lines.len()
        )
        .unwrap_or_default();
    }
    Ran::done(content)
}

#[cfg(test)]
mod tests {
    use super::super::scratch::Scratch;
    use super::*;

    #[test]
    fn lines_come_numbered_from_the_offset_with_a_note_to_continue() {
        let scratch = Scratch::new();
        let file = scratch.file("a.txt", "one\ntwo\nthree\nfour\n");
        let ran = read(&file, "a.txt", Some(2), Some(2));
        assert!(!ran.failed);
        assert_eq!(
            ran.content,
            "     2\ttwo\n     3\tthree\n[2 of 4 lines shown; continue with offset 4]\n"
        );
        let whole = read(&file, "a.txt", None, None);
        assert!(whole.content.ends_with("     4\tfour\n"));
        assert!(read(&file, "a.txt", Some(9), None).failed);
    }

    #[test]
    fn long_lines_are_cut_and_empty_files_say_so() {
        let scratch = Scratch::new();
        let long = scratch.file("long.txt", &"x".repeat(MAX_LINE_CHARS + 5));
        assert!(read(&long, "long.txt", None, None)
            .content
            .contains(LINE_CUT_MARK));
        let empty = scratch.file("empty.txt", "");
        assert_eq!(read(&empty, "empty.txt", None, None).content, EMPTY_TEXT);
    }

    #[test]
    fn binary_files_and_folders_are_refused() {
        let scratch = Scratch::new();
        let binary = scratch.path().join("blob.bin");
        std::fs::write(&binary, [1, 2, NUL, 3]).unwrap_or_default();
        let ran = read(&binary, "blob.bin", None, None);
        assert!(ran.failed && ran.content.contains(BINARY_TEXT));
        let folder = read(scratch.path(), "here", None, None);
        assert!(folder.failed && folder.content.contains(DIRECTORY_TEXT));
        assert!(read(&scratch.path().join("missing"), "missing", None, None).failed);
    }
}
