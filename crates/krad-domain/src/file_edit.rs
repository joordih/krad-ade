use serde::{Deserialize, Serialize};

const MAX_DIFF_CELLS: usize = 250_000;
const LINE_END: char = '\n';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineChange {
    Kept,
    Removed,
    Added,
    Gap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditLine {
    pub change: LineChange,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_line: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEdit {
    pub path: String,
    pub lines: Vec<EditLine>,
}

impl FileEdit {
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        let count = |change| {
            self.lines
                .iter()
                .filter(|line| line.change == change)
                .count()
        };
        (count(LineChange::Added), count(LineChange::Removed))
    }
}

#[must_use]
pub fn line_diff(old: &str, new: &str) -> Vec<EditLine> {
    let old: Vec<&str> = split_lines(old);
    let new: Vec<&str> = split_lines(new);
    let line = |change, text: &str| EditLine {
        change,
        text: text.to_owned(),
        old_line: None,
        new_line: None,
    };
    if old.len().saturating_mul(new.len()) > MAX_DIFF_CELLS {
        return old
            .iter()
            .map(|text| line(LineChange::Removed, text))
            .chain(new.iter().map(|text| line(LineChange::Added, text)))
            .collect();
    }
    let kept = common_lengths(&old, &new);
    let (mut at_old, mut at_new) = (0, 0);
    let mut lines = Vec::with_capacity(old.len() + new.len());
    while at_old < old.len() || at_new < new.len() {
        if at_old < old.len() && at_new < new.len() && old[at_old] == new[at_new] {
            lines.push(line(LineChange::Kept, old[at_old]));
            at_old += 1;
            at_new += 1;
        } else if at_new < new.len()
            && (at_old == old.len() || kept[at_old][at_new + 1] >= kept[at_old + 1][at_new])
        {
            lines.push(line(LineChange::Added, new[at_new]));
            at_new += 1;
        } else {
            lines.push(line(LineChange::Removed, old[at_old]));
            at_old += 1;
        }
    }
    reorder_removals_first(lines)
}

fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    text.strip_suffix(LINE_END)
        .unwrap_or(text)
        .split(LINE_END)
        .collect()
}

fn common_lengths(old: &[&str], new: &[&str]) -> Vec<Vec<usize>> {
    let mut kept = vec![vec![0; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            kept[i][j] = if old[i] == new[j] {
                kept[i + 1][j + 1] + 1
            } else {
                kept[i + 1][j].max(kept[i][j + 1])
            };
        }
    }
    kept
}

fn reorder_removals_first(lines: Vec<EditLine>) -> Vec<EditLine> {
    let mut ordered = Vec::with_capacity(lines.len());
    let mut added = Vec::new();
    for line in lines {
        match line.change {
            LineChange::Added => added.push(line),
            LineChange::Removed => ordered.push(line),
            LineChange::Kept | LineChange::Gap => {
                ordered.append(&mut added);
                ordered.push(line);
            }
        }
    }
    ordered.append(&mut added);
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(lines: &[EditLine]) -> String {
        lines
            .iter()
            .map(|line| {
                let mark = match line.change {
                    LineChange::Kept | LineChange::Gap => ' ',
                    LineChange::Removed => '-',
                    LineChange::Added => '+',
                };
                format!("{mark}{}", line.text)
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn an_edit_keeps_the_unchanged_lines_around_the_change() {
        let lines = line_diff("a\nb\nc\n", "a\nB\nc\nd\n");
        assert_eq!(marks(&lines), " a|-b|+B| c|+d");
        let edit = FileEdit {
            path: "x".to_owned(),
            lines,
        };
        assert_eq!(edit.counts(), (2, 1));
    }

    #[test]
    fn a_new_file_is_all_added_and_a_deleted_text_all_removed() {
        assert_eq!(marks(&line_diff("", "one\ntwo")), "+one|+two");
        assert_eq!(marks(&line_diff("gone", "")), "-gone");
    }
}
