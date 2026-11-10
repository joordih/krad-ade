mod parse;

use krad_domain::{GitObjectId, HunkFingerprint};
use std::path::PathBuf;

use crate::{ChangeKind, LineCount};

pub use parse::parse_patch;

const NEWLINE: &str = "\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchStatus {
    Added,
    Deleted,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HunkLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HunkLine {
    pub kind: HunkLineKind,
    pub text: String,
    pub newline: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<HunkLine>,
    pub fingerprint: HunkFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchBody {
    Text(Vec<Hunk>),
    Binary,
    Submodule,
    Renamed { from: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePatch {
    pub path: PathBuf,
    pub status: PatchStatus,
    pub old_id: Option<GitObjectId>,
    pub new_id: Option<GitObjectId>,
    pub body: PatchBody,
    pub fingerprint: HunkFingerprint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaleHunk;

impl From<PatchStatus> for ChangeKind {
    fn from(status: PatchStatus) -> Self {
        match status {
            PatchStatus::Added => Self::Added,
            PatchStatus::Deleted => Self::Deleted,
            PatchStatus::Modified => Self::Modified,
        }
    }
}

impl FilePatch {
    #[must_use]
    pub fn hunks(&self) -> &[Hunk] {
        match &self.body {
            PatchBody::Text(hunks) => hunks,
            PatchBody::Binary | PatchBody::Submodule | PatchBody::Renamed { .. } => &[],
        }
    }

    #[must_use]
    pub fn fingerprints(&self) -> Vec<HunkFingerprint> {
        match &self.body {
            PatchBody::Text(hunks) if !hunks.is_empty() => {
                hunks.iter().map(|hunk| hunk.fingerprint).collect()
            }
            PatchBody::Text(_)
            | PatchBody::Binary
            | PatchBody::Submodule
            | PatchBody::Renamed { .. } => {
                vec![self.fingerprint]
            }
        }
    }

    #[must_use]
    pub fn line_count(&self) -> LineCount {
        self.hunks().iter().flat_map(|hunk| &hunk.lines).fold(
            LineCount::default(),
            |count, line| match line.kind {
                HunkLineKind::Context => count,
                HunkLineKind::Added => {
                    count
                        + LineCount {
                            added: 1,
                            removed: 0,
                        }
                }
                HunkLineKind::Removed => {
                    count
                        + LineCount {
                            added: 0,
                            removed: 1,
                        }
                }
            },
        )
    }
}

impl Hunk {
    fn side(&self, kept: HunkLineKind) -> Vec<(&str, bool)> {
        self.lines
            .iter()
            .filter(|line| line.kind == HunkLineKind::Context || line.kind == kept)
            .map(|line| (line.text.as_str(), line.newline))
            .collect()
    }

    fn anchor(&self, new_len: usize) -> usize {
        if new_len == 0 {
            self.new_start
        } else {
            self.new_start.saturating_sub(1)
        }
    }
}

fn split_lines(text: &str) -> Vec<(&str, bool)> {
    text.split_inclusive(NEWLINE)
        .map(|line| match line.strip_suffix(NEWLINE) {
            Some(content) => (content, true),
            None => (line, false),
        })
        .collect()
}

fn nearest_match(
    current: &[(&str, bool)],
    wanted: &[(&str, bool)],
    anchor: usize,
) -> Option<usize> {
    let last = current.len().checked_sub(wanted.len())?;
    let fits = |at: usize| current.get(at..at + wanted.len()) == Some(wanted);
    if wanted.is_empty() {
        return (anchor <= last).then_some(anchor);
    }
    (0..=last.max(anchor)).find_map(|distance| {
        let before = anchor.checked_sub(distance).filter(|at| fits(*at));
        let after = anchor
            .checked_add(distance)
            .filter(|at| distance > 0 && *at <= last && fits(*at));
        before.or(after)
    })
}

/// # Errors
///
/// Returns [`StaleHunk`] when the lines the hunk produced are no longer in
/// `current`, so reverting it would touch something else.
pub fn revert_hunk(current: &str, hunk: &Hunk) -> Result<String, StaleHunk> {
    let lines = split_lines(current);
    let new_side = hunk.side(HunkLineKind::Added);
    let old_side = hunk.side(HunkLineKind::Removed);
    let at = nearest_match(&lines, &new_side, hunk.anchor(new_side.len())).ok_or(StaleHunk)?;
    let mut text = String::with_capacity(current.len());
    let before = lines.get(..at).unwrap_or_default();
    let after = lines.get(at + new_side.len()..).unwrap_or_default();
    for (content, newline) in before.iter().chain(&old_side).chain(after) {
        text.push_str(content);
        if *newline {
            text.push_str(NEWLINE);
        }
    }
    Ok(text)
}

/// # Errors
///
/// Returns [`StaleHunk`] when any hunk no longer matches `current`.
pub fn revert_hunks(current: &str, hunks: &[Hunk]) -> Result<String, StaleHunk> {
    hunks
        .iter()
        .rev()
        .try_fold(current.to_owned(), |text, hunk| revert_hunk(&text, hunk))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -1,3 +1,3 @@\n",
        " one\n",
        "-two\n",
        "+TWO\n",
        " three\n",
        "@@ -8,3 +8,4 @@ fn main\n",
        " eight\n",
        " nine\n",
        "+nine and a half\n",
        " ten\n",
    );

    const AFTER: &str =
        "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nnine\nnine and a half\nten\n";

    fn hunks() -> Vec<Hunk> {
        parse_patch(DIFF)
            .into_iter()
            .next()
            .map_or_else(Vec::new, |file| file.hunks().to_vec())
    }

    #[test]
    fn reverting_one_hunk_leaves_the_others_and_the_rest_of_the_file() {
        let hunks = hunks();
        assert_eq!(
            revert_hunk(AFTER, &hunks[1]).as_deref(),
            Ok("one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n")
        );
        assert_eq!(
            revert_hunk(AFTER, &hunks[0]).as_deref(),
            Ok("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nnine and a half\nten\n")
        );
    }

    #[test]
    fn a_hunk_is_found_again_after_lines_moved_above_it() {
        let hunks = hunks();
        let moved = format!("zero\nzero\n{AFTER}");
        assert_eq!(
            revert_hunk(&moved, &hunks[1]).as_deref(),
            Ok("zero\nzero\none\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n")
        );
    }

    #[test]
    fn a_hunk_whose_lines_were_edited_since_is_stale() {
        let hunks = hunks();
        let edited = AFTER.replace("TWO", "Two");
        assert_eq!(revert_hunk(&edited, &hunks[0]), Err(StaleHunk));
        assert_eq!(
            revert_hunk(&edited, &hunks[1]).map(|text| text.contains("Two")),
            Ok(true)
        );
    }

    #[test]
    fn reverting_every_hunk_gives_back_the_old_file() {
        assert_eq!(
            revert_hunks(AFTER, &hunks()).as_deref(),
            Ok("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n")
        );
    }

    #[test]
    fn a_missing_final_newline_and_carriage_returns_are_kept() {
        let diff = concat!(
        "diff --git a/a.txt b/a.txt\n",
        "index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n",
        "--- a/a.txt\n",
        "+++ b/a.txt\n",
        "@@ -1,2 +1,2 @@\n",
        " a\r\n",
        "-b\r\n",
        "\\ No newline at end of file\n",
        "+c\r\n",
    );
        let hunks: Vec<Hunk> = parse_patch(diff)
            .into_iter()
            .flat_map(|file| file.hunks().to_vec())
            .collect();
        assert_eq!(
            revert_hunks("a\r\nc\r\n", &hunks).as_deref(),
            Ok("a\r\nb\r")
        );
    }

    #[test]
    fn line_counts_and_fingerprints_follow_the_hunks() {
        let file = parse_patch(DIFF).remove(0);
        assert_eq!(
            file.line_count(),
            LineCount {
                added: 2,
                removed: 1
            }
        );
        let prints = file.fingerprints();
        assert_eq!(prints.len(), 2);
        assert_ne!(prints[0], prints[1]);
        let shifted = DIFF.replace("@@ -8,3 +8,4 @@", "@@ -9,3 +9,4 @@");
        assert_eq!(parse_patch(&shifted).remove(0).fingerprints(), prints);
    }
}
