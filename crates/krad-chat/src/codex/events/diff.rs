use krad_domain::{line_diff, EditLine, FileEdit, LineChange};
use serde::Deserialize;

const HUNK_MARK: &str = "@@";
const OLD_FILE_MARK: &str = "--- ";
const NEW_FILE_MARK: &str = "+++ ";
const KEPT_MARK: char = ' ';
const REMOVED_MARK: char = '-';
const ADDED_MARK: char = '+';

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum ChangeKind {
    Add,
    Delete,
    Update,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FileUpdate {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) diff: String,
    pub(crate) kind: ChangeKind,
}

fn numbered(diff: &str) -> Vec<EditLine> {
    let (mut old, mut new) = (0_u32, 0_u32);
    let mut lines = Vec::new();
    for line in diff.lines() {
        if line.starts_with(OLD_FILE_MARK) || line.starts_with(NEW_FILE_MARK) {
            continue;
        }
        if let Some(header) = line.strip_prefix(HUNK_MARK) {
            if !lines.is_empty() {
                lines.push(EditLine {
                    change: LineChange::Gap,
                    text: String::new(),
                    old_line: None,
                    new_line: None,
                });
            }
            let mut starts = header
                .split_whitespace()
                .filter_map(|range| range.get(1..)?.split(',').next()?.parse::<u32>().ok());
            old = starts.next().unwrap_or(1);
            new = starts.next().unwrap_or(1);
            continue;
        }
        let mut chars = line.chars();
        let Some(mark) = chars.next() else {
            continue;
        };
        let text = chars.as_str().to_owned();
        let (change, old_line, new_line) = match mark {
            KEPT_MARK => (LineChange::Kept, Some(old), Some(new)),
            REMOVED_MARK => (LineChange::Removed, Some(old), None),
            ADDED_MARK => (LineChange::Added, None, Some(new)),
            _ => continue,
        };
        old += u32::from(old_line.is_some());
        new += u32::from(new_line.is_some());
        lines.push(EditLine {
            change,
            text,
            old_line,
            new_line,
        });
    }
    lines
}

pub(super) fn file_edit(update: &FileUpdate) -> Option<FileEdit> {
    let lines = if update.diff.contains(HUNK_MARK) {
        numbered(&update.diff)
    } else {
        match update.kind {
            ChangeKind::Add => line_diff("", &update.diff),
            ChangeKind::Delete => line_diff(&update.diff, ""),
            ChangeKind::Update => Vec::new(),
        }
    };
    (!lines.is_empty()).then(|| FileEdit {
        path: update.path.clone(),
        lines,
    })
}
