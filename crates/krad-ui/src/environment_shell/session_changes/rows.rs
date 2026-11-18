use gpui_kit::SharedString;
use krad_domain::{
    FilePatch, Hunk, HunkFingerprint, HunkLine, HunkLineKind, LineCount, PatchBody, SessionChanges,
};

use super::super::diff_view::{change_runs, line_position, with_headers, ChangeRun};
use super::super::review_view::{DiffLine, LineKind};
use gpui_kit::component::diff::DiffLinePosition;

const CONTEXT_MARK: char = ' ';
const ADDED_MARK: char = '+';
const REMOVED_MARK: char = '-';
const NO_NEWLINE: &str = "\\ No newline at end of file";
const BINARY_FILES: &str = "Binary files differ";
const RENAME_FROM: &str = "rename from ";
const RENAME_TO: &str = "rename to ";
const INDEX: &str = "index ";
const INDEX_RANGE: &str = "..";
const SUBMODULE_MODE: &str = " 160000";

pub(super) struct ReviewedFile {
    pub(super) patch: FilePatch,
    pub(super) path: SharedString,
    pub(super) prints: Vec<HunkFingerprint>,
    pub(super) count: LineCount,
    pub(super) hunks: Vec<Vec<DiffLine>>,
    pub(super) runs: Vec<ChangeRun>,
}

impl From<FilePatch> for ReviewedFile {
    fn from(patch: FilePatch) -> Self {
        let hunks: Vec<Vec<DiffLine>> = patch.hunks().iter().map(hunk_lines).collect();
        Self {
            path: SharedString::from(patch.path.to_string_lossy().into_owned()),
            prints: patch.fingerprints(),
            count: patch.line_count(),
            runs: change_runs(hunks.iter().flatten()),
            hunks,
            patch,
        }
    }
}

impl ReviewedFile {
    pub(super) fn hunk_end(&self, hunk: usize) -> Option<DiffLinePosition> {
        line_position(&self.path, self.hunks.get(hunk)?.last()?)
    }

    pub(super) fn hunk_at(&self, position: &DiffLinePosition) -> Option<usize> {
        (0..self.hunks.len()).find(|hunk| self.hunk_end(*hunk).as_ref() == Some(position))
    }
}

fn push_line(text: &mut String, line: &HunkLine) {
    let mark = match line.kind {
        HunkLineKind::Context => CONTEXT_MARK,
        HunkLineKind::Added => ADDED_MARK,
        HunkLineKind::Removed => REMOVED_MARK,
    };
    text.push(mark);
    text.push_str(&line.text);
    text.push('\n');
    if !line.newline {
        text.push_str(NO_NEWLINE);
        text.push('\n');
    }
}

fn push_hunk(text: &mut String, hunk: &Hunk) {
    let count = |kind| {
        hunk.lines
            .iter()
            .filter(|line| line.kind == HunkLineKind::Context || line.kind == kind)
            .count()
    };
    let old = count(HunkLineKind::Removed);
    let new = count(HunkLineKind::Added);
    let header = format!(
        "@@ -{},{old} +{},{new} @@
",
        hunk.old_start, hunk.new_start
    );
    text.push_str(&header);
    for line in &hunk.lines {
        push_line(text, line);
    }
}

#[must_use]
pub(super) fn display_patch(file: &FilePatch) -> String {
    let path = file.path.to_string_lossy();
    let mut old = path.clone();
    let mut body = String::new();
    match &file.body {
        PatchBody::Text(hunks) => hunks.iter().for_each(|hunk| push_hunk(&mut body, hunk)),
        PatchBody::Binary => body = format!("{BINARY_FILES}\n"),
        PatchBody::Renamed { from } => {
            old = from.to_string_lossy();
            body = format!("{RENAME_FROM}{old}\n{RENAME_TO}{path}\n");
        }
        PatchBody::Submodule => {
            if let (Some(old), Some(new)) = (&file.old_id, &file.new_id) {
                body = format!("{INDEX}{old}{INDEX_RANGE}{new}{SUBMODULE_MODE}\n");
            }
        }
    }
    with_headers(&old, &path, &body)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileRow {
    pub(super) file: usize,
    pub(super) reviewed: bool,
    pub(super) hunks: Vec<bool>,
}

fn line_number(value: usize) -> Option<u32> {
    u32::try_from(value).ok()
}

pub(super) fn hunk_lines(hunk: &Hunk) -> Vec<DiffLine> {
    let (mut old, mut new) = (hunk.old_start, hunk.new_start);
    hunk.lines
        .iter()
        .map(|line| {
            let (kind, old_number, new_number) = match line.kind {
                HunkLineKind::Context => {
                    let numbers = (Some(old), Some(new));
                    old += 1;
                    new += 1;
                    (LineKind::Context, numbers.0, numbers.1)
                }
                HunkLineKind::Removed => {
                    old += 1;
                    (LineKind::Removed, Some(old - 1), None)
                }
                HunkLineKind::Added => {
                    new += 1;
                    (LineKind::Added, None, Some(new - 1))
                }
            };
            DiffLine {
                kind,
                old: old_number.and_then(line_number),
                new: new_number.and_then(line_number),
                text: line.text.clone(),
            }
        })
        .collect()
}

pub(super) fn review_rows(
    files: &[ReviewedFile],
    changes: Option<&SessionChanges>,
) -> Vec<FileRow> {
    let reviewed = |prints: &[HunkFingerprint]| changes.is_some_and(|c| c.is_reviewed(prints));
    files
        .iter()
        .enumerate()
        .map(|(file, reviewed_file)| FileRow {
            file,
            reviewed: reviewed(&reviewed_file.prints),
            hunks: reviewed_file
                .patch
                .hunks()
                .iter()
                .map(|hunk| reviewed(&[hunk.fingerprint]))
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::super::diff_view::parse_files;
    use super::*;
    use gpui_kit::component::diff::DiffSide;
    use krad_domain::parse_patch;

    const DIFF: &str = concat!(
        "diff --git a/a.rs b/a.rs\n",
        "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
        "--- a/a.rs\n",
        "+++ b/a.rs\n",
        "@@ -1,2 +1,2 @@\n",
        " keep\n",
        "-old\n",
        "+new\n",
        "@@ -9,1 +9,2 @@\n",
        " nine\n",
        "+more\n",
        "diff --git a/b.rs b/b.rs\n",
        "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
        "--- a/b.rs\n",
        "+++ b/b.rs\n",
        "@@ -1 +1 @@\n",
        "-x\n",
        "+y\n",
    );

    fn files() -> Vec<ReviewedFile> {
        parse_patch(DIFF)
            .into_iter()
            .map(ReviewedFile::from)
            .collect()
    }

    #[test]
    fn hunk_lines_are_numbered_on_both_sides() {
        let files = files();
        let numbers: Vec<(Option<u32>, Option<u32>)> = files[0].hunks[0]
            .iter()
            .map(|line| (line.old, line.new))
            .collect();
        assert_eq!(
            numbers,
            [(Some(1), Some(1)), (Some(2), None), (None, Some(2))]
        );
    }

    #[test]
    fn every_hunk_reports_its_review_and_accepting_every_hunk_accepts_the_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let files = files();
        let mut changes = SessionChanges::new("4b825dc642cb6eb9a060e54bf8d69288fbee4904".parse()?);
        changes.set_reviewed(&[files[0].patch.hunks()[0].fingerprint], true);
        assert_eq!(
            review_rows(&files, Some(&changes)),
            [
                FileRow {
                    file: 0,
                    reviewed: false,
                    hunks: vec![true, false],
                },
                FileRow {
                    file: 1,
                    reviewed: false,
                    hunks: vec![false],
                },
            ]
        );
        changes.set_reviewed(&files[0].prints, true);
        assert!(review_rows(&files, Some(&changes))[0].reviewed);
        Ok(())
    }

    #[test]
    fn a_patch_is_shown_as_one_file_with_its_hunks_and_their_last_lines(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let files = files();
        let shown = parse_files(&display_patch(&files[0].patch))?;
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].path().as_str(), "a.rs");
        assert_eq!((shown[0].additions(), shown[0].deletions()), (2, 1));
        let end = files[0].hunk_end(1).ok_or("the second hunk has lines")?;
        assert_eq!((end.side(), end.line()), (DiffSide::Modified, 10));
        assert_eq!(files[0].hunk_at(&end), Some(1));
        Ok(())
    }

    #[test]
    fn a_line_without_its_newline_is_marked_and_a_rename_keeps_both_paths(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unterminated = FilePatch {
            body: PatchBody::Text(vec![Hunk {
                old_start: 1,
                new_start: 1,
                lines: vec![
                    HunkLine {
                        kind: HunkLineKind::Removed,
                        text: "old".to_owned(),
                        newline: false,
                    },
                    HunkLine {
                        kind: HunkLineKind::Added,
                        text: "new".to_owned(),
                        newline: false,
                    },
                ],
                fingerprint: files()[0].patch.fingerprint,
            }]),
            ..files().remove(0).patch
        };
        let text = display_patch(&unterminated);
        assert!(text.ends_with("+new\n\\ No newline at end of file\n"));
        assert_eq!(parse_files(&text)?[0].additions(), 1);
        let renamed = FilePatch {
            body: PatchBody::Renamed {
                from: "old.rs".into(),
            },
            ..files().remove(0).patch
        };
        let shown = parse_files(&display_patch(&renamed))?;
        assert_eq!(
            shown[0].original_path().map(SharedString::as_str),
            Some("old.rs")
        );
        assert_eq!(shown[0].path().as_str(), "a.rs");
        Ok(())
    }
}
