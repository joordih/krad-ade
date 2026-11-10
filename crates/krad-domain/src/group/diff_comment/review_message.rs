use std::path::Path;

use super::{AnchorState, DiffComment, DiffSide, LineSpan};

const INTRO: &str =
    "Review of your changes in this worktree. Address each comment below, then reply with what you changed.";
const DIFF_FENCE_OPEN: &str = "```diff";
const FENCE_CLOSE: &str = "```";
const LINE_BREAK: &str = "\n";
const ENTRY_GAP: &str = "\n\n";
const LINE_WORD: &str = "line";
const LINES_WORD: &str = "lines";
const OLD_SIDE_NOTE: &str = " of the previous version";
const STALE_NOTE: &str = " (the code changed after this comment; check it still applies)";

fn span_label(span: LineSpan) -> String {
    let side = match span.side {
        DiffSide::New => "",
        DiffSide::Old => OLD_SIDE_NOTE,
    };
    if span.first == span.last {
        format!("{LINE_WORD} {}{side}", span.first)
    } else {
        format!("{LINES_WORD} {}-{}{side}", span.first, span.last)
    }
}

fn entry(number: usize, root: &Path, comment: &DiffComment) -> String {
    let path = &comment.quote.anchor.path;
    let shown = path.strip_prefix(root).unwrap_or(path);
    let stale = match comment.state {
        AnchorState::Current => "",
        AnchorState::Stale => STALE_NOTE,
    };
    let excerpt = comment.quote.excerpt.join(LINE_BREAK);
    format!(
        "{number}. {}, {}{stale}:\n{DIFF_FENCE_OPEN}\n{excerpt}\n{FENCE_CLOSE}\n{}",
        shown.display(),
        span_label(comment.quote.span),
        comment.text.trim(),
    )
}

#[must_use]
pub fn review_message(root: &Path, comments: &[DiffComment]) -> String {
    let entries = comments
        .iter()
        .enumerate()
        .map(|(index, comment)| entry(index + 1, root, comment));
    std::iter::once(INTRO.to_owned())
        .chain(entries)
        .collect::<Vec<_>>()
        .join(ENTRY_GAP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::{DiffAnchor, DiffCommentId, DiffQuote, HunkLines};
    use std::path::PathBuf;

    fn comment(
        id: u64,
        path: &str,
        span: LineSpan,
        excerpt: &[&str],
        text: &str,
        state: AnchorState,
    ) -> DiffComment {
        DiffComment {
            id: DiffCommentId(id),
            quote: DiffQuote {
                anchor: DiffAnchor {
                    path: PathBuf::from(path),
                    hunk: excerpt.iter().copied().collect(),
                    lines: HunkLines::new(0, excerpt.len() - 1),
                },
                span,
                excerpt: excerpt.iter().map(|line| (*line).to_owned()).collect(),
            },
            text: text.to_owned(),
            state,
        }
    }

    #[test]
    fn one_message_lists_every_comment_with_its_file_lines_and_diff() {
        let comments = [
            comment(
                1,
                "/w/api/src/lib.rs",
                LineSpan {
                    side: DiffSide::New,
                    first: 12,
                    last: 13,
                },
                &["-let a = 1;", "+let a = 2;", " call(a);"],
                "  Why two?\n",
                AnchorState::Current,
            ),
            comment(
                2,
                "/w/api/src/main.rs",
                LineSpan {
                    side: DiffSide::Old,
                    first: 4,
                    last: 4,
                },
                &["-dead();"],
                "Keep this call.",
                AnchorState::Stale,
            ),
        ];
        let message = review_message(Path::new("/w/api"), &comments);
        assert_eq!(
            message,
            format!(
                "{INTRO}\n\n\
                 1. src/lib.rs, lines 12-13:\n```diff\n-let a = 1;\n+let a = 2;\n call(a);\n```\nWhy two?\n\n\
                 2. src/main.rs, line 4 of the previous version{STALE_NOTE}:\n```diff\n-dead();\n```\nKeep this call."
            )
        );
    }

    #[test]
    fn a_path_outside_the_root_is_shown_whole() {
        let comments = [comment(
            1,
            "/elsewhere/x.rs",
            LineSpan {
                side: DiffSide::New,
                first: 1,
                last: 1,
            },
            &["+x"],
            "ok",
            AnchorState::Current,
        )];
        let message = review_message(Path::new("/w/api"), &comments);
        assert!(message.contains("1. /elsewhere/x.rs, line 1:"));
    }
}
