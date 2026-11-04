use super::super::review_view::{DiffLine, LineKind};

const CRLF: &str = "\r\n";
const LF: &str = "\n";

const fn is_change(kind: LineKind) -> bool {
    matches!(kind, LineKind::Added | LineKind::Removed)
}

pub(super) fn block_starts(lines: &[DiffLine]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            is_change(line.kind)
                && index
                    .checked_sub(1)
                    .and_then(|before| lines.get(before))
                    .is_none_or(|before| !is_change(before.kind))
        })
        .map(|(index, _)| index)
        .collect()
}

fn block_end(lines: &[DiffLine], start: usize) -> usize {
    (start..lines.len())
        .find(|index| !is_change(lines[*index].kind))
        .unwrap_or(lines.len())
}

pub(in crate::environment_shell) fn reverted_block(
    lines: &[DiffLine],
    start: usize,
    current: &str,
) -> Option<String> {
    let worktree = lines
        .iter()
        .filter(|line| matches!(line.kind, LineKind::Context | LineKind::Added))
        .map(|line| line.text.as_str());
    if !worktree.eq(current.lines()) {
        return None;
    }
    let end = block_end(lines, start);
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| match line.kind {
            LineKind::Hunk => false,
            LineKind::Context => true,
            LineKind::Removed => (start..end).contains(index),
            LineKind::Added => !(start..end).contains(index),
        })
        .map(|(_, line)| line.text.as_str())
        .collect();
    if kept.is_empty() {
        return Some(String::new());
    }
    let eol = if current.contains(CRLF) { CRLF } else { LF };
    let mut text = kept.join(eol);
    if current.is_empty() || current.ends_with(LF) {
        text.push_str(eol);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::super::super::review_view::numbered_lines;
    use super::*;

    const DIFF: &str = "@@ -1,4 +1,4 @@\n one\n-two\n+TWO\n three\n-four\n+FOUR\n";

    #[test]
    fn each_run_of_changed_lines_is_one_block() {
        assert_eq!(block_starts(&numbered_lines(DIFF)), [2, 5]);
    }

    #[test]
    fn reverting_a_block_restores_its_removed_lines_and_keeps_the_rest() {
        let lines = numbered_lines(DIFF);
        let current = "one\nTWO\nthree\nFOUR\n";
        assert_eq!(
            reverted_block(&lines, 5, current).as_deref(),
            Some("one\nTWO\nthree\nfour\n")
        );
        assert_eq!(
            reverted_block(&lines, 2, &current.replace('\n', "\r\n")).as_deref(),
            Some("one\r\ntwo\r\nthree\r\nFOUR\r\n")
        );
    }

    #[test]
    fn a_file_edited_since_the_diff_was_read_is_left_alone() {
        let lines = numbered_lines(DIFF);
        assert_eq!(reverted_block(&lines, 2, "one\nTWO\nthree\nFIVE\n"), None);
    }
}
