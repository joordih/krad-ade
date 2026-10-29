use krad_domain::review::{Change, NotePosition};

use super::words::HostWords;

const HUNK_PREFIX: &str = "@@";
const ADDED_PREFIX: char = '+';
const REMOVED_PREFIX: char = '-';
const CONTEXT_PREFIX: char = ' ';
const NO_NEWLINE_PREFIX: char = '\\';
const OLD_RANGE_PREFIX: char = '-';
const NEW_RANGE_PREFIX: char = '+';
const RANGE_COUNT_SEPARATOR: char = ',';
const HUNK_FIELD_SEPARATOR: char = ' ';

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::environment_shell) enum LineKind {
    Hunk,
    Context,
    Added,
    Removed,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::environment_shell) struct DiffLine {
    pub(in crate::environment_shell) kind: LineKind,
    pub(super) old: Option<u32>,
    pub(super) new: Option<u32>,
    pub(in crate::environment_shell) text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DiffSource {
    CodeHost,
    LocalCheckout,
}

const LOCAL_CHECKOUT_LABEL: &str = "from local checkout";

impl DiffSource {
    pub(super) const fn label(self, words: &HostWords) -> &'static str {
        match self {
            Self::CodeHost => words.diff_from,
            Self::LocalCheckout => LOCAL_CHECKOUT_LABEL,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::environment_shell) struct ChangedFile {
    pub(super) change: Change,
    pub(super) lines: Vec<DiffLine>,
    pub(super) added: usize,
    pub(super) removed: usize,
    pub(super) source: DiffSource,
}

impl ChangedFile {
    pub(super) fn new(change: Change) -> Self {
        let lines = numbered_lines(&change.diff);
        let mut file = Self {
            change,
            lines: Vec::new(),
            added: 0,
            removed: 0,
            source: DiffSource::CodeHost,
        };
        file.set_lines(lines, DiffSource::CodeHost);
        file
    }

    pub(super) fn set_diff(&mut self, diff: &str, source: DiffSource) {
        self.set_lines(numbered_lines(diff), source);
    }

    fn set_lines(&mut self, lines: Vec<DiffLine>, source: DiffSource) {
        let count = |kind| lines.iter().filter(|line| line.kind == kind).count();
        self.added = count(LineKind::Added);
        self.removed = count(LineKind::Removed);
        self.lines = lines;
        self.source = source;
    }

    pub(super) fn name(&self) -> &str {
        self.change
            .new_path
            .rsplit('/')
            .next()
            .unwrap_or(&self.change.new_path)
    }

    pub(super) fn holds(&self, position: &NotePosition) -> bool {
        position.new_path.as_deref() == Some(self.change.new_path.as_str())
            || position.old_path.as_deref() == Some(self.change.old_path.as_str())
    }
}

pub(in crate::environment_shell) fn numbered_lines(diff: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let (mut old, mut new) = (0, 0);
    for line in diff.lines() {
        if line.starts_with(HUNK_PREFIX) {
            if let Some((old_start, new_start)) = hunk_starts(line) {
                (old, new) = (old_start, new_start);
            }
            lines.push(DiffLine {
                kind: LineKind::Hunk,
                old: None,
                new: None,
                text: line.to_owned(),
            });
            continue;
        }
        let mut chars = line.chars();
        let (kind, numbers) = match chars.next() {
            Some(ADDED_PREFIX) => (LineKind::Added, (None, Some(new))),
            Some(REMOVED_PREFIX) => (LineKind::Removed, (Some(old), None)),
            Some(NO_NEWLINE_PREFIX) => continue,
            Some(CONTEXT_PREFIX) | None => (LineKind::Context, (Some(old), Some(new))),
            Some(_) => {
                chars = line.chars();
                (LineKind::Context, (Some(old), Some(new)))
            }
        };
        if numbers.0.is_some() {
            old += 1;
        }
        if numbers.1.is_some() {
            new += 1;
        }
        lines.push(DiffLine {
            kind,
            old: numbers.0,
            new: numbers.1,
            text: chars.as_str().to_owned(),
        });
    }
    lines
}

fn hunk_starts(header: &str) -> Option<(u32, u32)> {
    let mut fields = header
        .trim_start_matches(HUNK_PREFIX)
        .split(HUNK_FIELD_SEPARATOR)
        .filter(|field| !field.is_empty());
    let old = range_start(fields.next()?.strip_prefix(OLD_RANGE_PREFIX)?)?;
    let new = range_start(fields.next()?.strip_prefix(NEW_RANGE_PREFIX)?)?;
    Some((old, new))
}

fn range_start(range: &str) -> Option<u32> {
    range
        .split(RANGE_COUNT_SEPARATOR)
        .next()
        .and_then(|start| start.parse().ok())
}

pub(super) fn anchor_line(lines: &[DiffLine], position: &NotePosition) -> Option<usize> {
    if let Some(new) = position.new_line {
        return lines.iter().position(|line| line.new == Some(new));
    }
    let old = position.old_line?;
    lines.iter().position(|line| line.old == Some(old))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "@@ -18,4 +18,5 @@ fn handle()\n \
                        let secret = 1;\n\
                        -let event = parse();\n\
                        +let signature = sign();\n\
                        +let event = verify();\n \
                        dispatch();\n\
                        @@ -40 +41 @@\n\
                        -old\n\
                        +new\n\
                        \\ No newline at end of file\n";

    fn numbers(lines: &[DiffLine]) -> Vec<(LineKind, Option<u32>, Option<u32>)> {
        lines
            .iter()
            .map(|line| (line.kind, line.old, line.new))
            .collect()
    }

    #[test]
    fn lines_count_up_from_each_hunk_start() {
        let lines = numbered_lines(DIFF);
        assert_eq!(
            numbers(&lines),
            [
                (LineKind::Hunk, None, None),
                (LineKind::Context, Some(18), Some(18)),
                (LineKind::Removed, Some(19), None),
                (LineKind::Added, None, Some(19)),
                (LineKind::Added, None, Some(20)),
                (LineKind::Context, Some(20), Some(21)),
                (LineKind::Hunk, None, None),
                (LineKind::Removed, Some(40), None),
                (LineKind::Added, None, Some(41)),
            ]
        );
        assert_eq!(lines[1].text, "let secret = 1;");
        assert_eq!(lines[3].text, "let signature = sign();");
        assert_eq!(lines[0].text, "@@ -18,4 +18,5 @@ fn handle()");
    }

    #[test]
    fn a_blank_context_line_still_counts_on_both_sides() {
        let lines = numbered_lines("@@ -1,3 +1,3 @@\n a\n\n b\n");
        assert_eq!(
            numbers(&lines)[2..],
            [
                (LineKind::Context, Some(2), Some(2)),
                (LineKind::Context, Some(3), Some(3))
            ]
        );
    }

    #[test]
    fn a_file_counts_its_added_and_removed_lines() {
        let file = ChangedFile::new(Change {
            old_path: "src/lib.rs".to_owned(),
            new_path: "src/lib.rs".to_owned(),
            diff: DIFF.to_owned(),
            new_file: false,
            renamed_file: false,
            deleted_file: false,
        });
        assert_eq!((file.added, file.removed), (3, 2));
        assert_eq!(file.name(), "lib.rs");
        assert_eq!(file.change.diff, DIFF);
        assert_eq!(file.source, DiffSource::CodeHost);
    }

    #[test]
    fn notes_hang_under_their_new_line_or_their_removed_old_line() {
        let lines = numbered_lines(DIFF);
        let at = |old_line, new_line| NotePosition {
            old_path: None,
            new_path: None,
            old_line,
            new_line,
        };
        assert_eq!(anchor_line(&lines, &at(None, Some(20))), Some(4));
        assert_eq!(anchor_line(&lines, &at(Some(19), None)), Some(2));
        assert_eq!(anchor_line(&lines, &at(Some(18), Some(18))), Some(1));
        assert_eq!(anchor_line(&lines, &at(None, Some(99))), None);
    }
}
