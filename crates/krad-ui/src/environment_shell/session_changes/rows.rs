use krad_domain::{HunkFingerprint, SessionChanges};
use krad_repo::{FilePatch, Hunk, HunkLineKind, LineCount};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use super::super::review_view::{DiffLine, LineKind};

pub(super) struct ReviewedFile {
    pub(super) patch: FilePatch,
    pub(super) prints: Vec<HunkFingerprint>,
    pub(super) count: LineCount,
    pub(super) hunks: Vec<Vec<DiffLine>>,
}

impl From<FilePatch> for ReviewedFile {
    fn from(patch: FilePatch) -> Self {
        Self {
            prints: patch.fingerprints(),
            count: patch.line_count(),
            hunks: patch.hunks().iter().map(hunk_lines).collect(),
            patch,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HunkRow {
    pub(super) hunk: usize,
    pub(super) reviewed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileRow {
    pub(super) file: usize,
    pub(super) reviewed: bool,
    pub(super) expanded: bool,
    pub(super) hunks: Vec<HunkRow>,
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

pub(super) fn present_prints(files: &[ReviewedFile]) -> BTreeSet<HunkFingerprint> {
    files
        .iter()
        .flat_map(|file| file.prints.iter().copied())
        .collect()
}

pub(super) fn review_rows(
    files: &[ReviewedFile],
    changes: Option<&SessionChanges>,
    expanded: &HashSet<PathBuf>,
) -> Vec<FileRow> {
    let reviewed = |prints: &[HunkFingerprint]| changes.is_some_and(|c| c.is_reviewed(prints));
    files
        .iter()
        .enumerate()
        .map(|(file, reviewed_file)| {
            let open = expanded.contains(&reviewed_file.patch.path);
            let hunks = if open {
                reviewed_file
                    .patch
                    .hunks()
                    .iter()
                    .enumerate()
                    .map(|(hunk, body)| HunkRow {
                        hunk,
                        reviewed: reviewed(&[body.fingerprint]),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            FileRow {
                file,
                reviewed: reviewed(&reviewed_file.prints),
                expanded: open,
                hunks,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::parse_patch;

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
    fn only_expanded_files_list_their_hunks_and_accepting_every_hunk_accepts_the_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let files = files();
        let mut changes = SessionChanges::new("4b825dc642cb6eb9a060e54bf8d69288fbee4904".parse()?);
        changes.set_reviewed(&[files[0].patch.hunks()[0].fingerprint], true);
        let expanded = HashSet::from([PathBuf::from("a.rs")]);
        assert_eq!(
            review_rows(&files, Some(&changes), &expanded),
            [
                FileRow {
                    file: 0,
                    reviewed: false,
                    expanded: true,
                    hunks: vec![
                        HunkRow {
                            hunk: 0,
                            reviewed: true
                        },
                        HunkRow {
                            hunk: 1,
                            reviewed: false
                        },
                    ],
                },
                FileRow {
                    file: 1,
                    reviewed: false,
                    expanded: false,
                    hunks: Vec::new(),
                },
            ]
        );
        changes.set_reviewed(&files[0].prints, true);
        assert!(review_rows(&files, Some(&changes), &HashSet::new())[0].reviewed);
        assert_eq!(present_prints(&files).len(), 3);
        Ok(())
    }
}
