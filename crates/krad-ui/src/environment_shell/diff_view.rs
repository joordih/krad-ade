use gpui_kit::component::diff::{
    Diff, DiffDecoration, DiffFile, DiffLinePosition, DiffLineRange, DiffMode, DiffParseError,
    DiffParseOptions, DiffSide, DiffState,
};
use gpui_kit::{
    div, AnyElement, App, AppContext as _, Context, ElementId, Entity, IntoElement, SharedString,
    Styled, WeakEntity,
};
use std::path::Path;

use super::review_view::{DiffLine, LineKind};
use super::EnvironmentShell;
use crate::components::{Button, IconName};
use crate::language::editor_id;
use crate::theme::{tinted, Palette};

const CHANGE_ALPHA: u32 = 0x26;
const OLD_HEADER: &str = "--- a/";
const NEW_HEADER: &str = "+++ b/";
const TOOLTIP_SPLIT: &str = "Show side by side";
const TOOLTIP_UNIFIED: &str = "Show in one column";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct ChangeRun {
    kind: LineKind,
    first: usize,
    last: usize,
}

impl ChangeRun {
    fn decoration(self, path: &SharedString, pal: &Palette) -> DiffDecoration {
        let (side, color) = match self.kind {
            LineKind::Removed => (DiffSide::Original, pal.diff_removed),
            LineKind::Added | LineKind::Context | LineKind::Hunk => {
                (DiffSide::Modified, pal.diff_added)
            }
        };
        DiffDecoration::new(DiffLineRange::new(
            path.clone(),
            side,
            self.first,
            self.last,
        ))
        .with_background(tinted(color, CHANGE_ALPHA))
    }
}

fn changed_number(line: &DiffLine) -> Option<usize> {
    let number = match line.kind {
        LineKind::Added => line.new,
        LineKind::Removed => line.old,
        LineKind::Context | LineKind::Hunk => None,
    }?;
    usize::try_from(number).ok()
}

#[must_use]
pub(in crate::environment_shell) fn change_runs<'a>(
    lines: impl IntoIterator<Item = &'a DiffLine>,
) -> Vec<ChangeRun> {
    let mut runs: Vec<ChangeRun> = Vec::new();
    let mut previous = None;
    for line in lines {
        let Some(number) = changed_number(line) else {
            previous = None;
            continue;
        };
        match runs.last_mut().filter(|_| previous == Some(line.kind)) {
            Some(run) => run.last = number,
            None => runs.push(ChangeRun {
                kind: line.kind,
                first: number,
                last: number,
            }),
        }
        previous = Some(line.kind);
    }
    runs
}

pub(in crate::environment_shell) fn change_decorations<'a>(
    path: &'a SharedString,
    runs: &'a [ChangeRun],
    pal: &'a Palette,
) -> impl Iterator<Item = DiffDecoration> + 'a {
    runs.iter().map(move |run| run.decoration(path, pal))
}

#[must_use]
pub(in crate::environment_shell) fn line_position(
    path: &SharedString,
    line: &DiffLine,
) -> Option<DiffLinePosition> {
    let (side, number) = match (line.new, line.old) {
        (Some(new), _) => (DiffSide::Modified, new),
        (None, Some(old)) => (DiffSide::Original, old),
        (None, None) => return None,
    };
    Some(DiffLinePosition::new(
        path.clone(),
        side,
        usize::try_from(number).ok()?,
    ))
}

#[must_use]
pub(in crate::environment_shell) fn line_index(
    lines: &[DiffLine],
    side: DiffSide,
    number: usize,
) -> Option<usize> {
    let number = u32::try_from(number).ok()?;
    lines.iter().position(|line| match side {
        DiffSide::Original => line.old == Some(number),
        DiffSide::Modified => line.new == Some(number),
    })
}

#[must_use]
pub(in crate::environment_shell) fn with_headers(old: &str, new: &str, hunks: &str) -> String {
    format!("{OLD_HEADER}{old}\n{NEW_HEADER}{new}\n{hunks}")
}

pub(in crate::environment_shell) fn parse_files(
    patch: &str,
) -> Result<Vec<DiffFile>, DiffParseError> {
    DiffFile::parse_with(patch, DiffParseOptions::new().truncated_hunks(true))
}

#[must_use]
pub(in crate::environment_shell) fn numbered_id(prefix: &'static str, number: u64) -> ElementId {
    ElementId::NamedInteger(prefix.into(), number)
}

#[must_use]
pub(in crate::environment_shell) fn id_number(id: &ElementId) -> Option<u64> {
    match id {
        ElementId::NamedInteger(_, number) => Some(*number),
        _ => None,
    }
}

pub(in crate::environment_shell) fn new_diff_state(
    files: Vec<DiffFile>,
    cx: &mut Context<EnvironmentShell>,
) -> Entity<DiffState> {
    cx.new(|cx| DiffState::new(files, cx))
}

#[must_use]
pub(in crate::environment_shell) fn themed_diff(state: &Entity<DiffState>) -> Diff {
    Diff::new(state).change_background(false).border_0()
}

#[must_use]
pub(in crate::environment_shell) fn mode_toggle(
    id: &'static str,
    state: &Entity<DiffState>,
    cx: &App,
) -> Button {
    let split = state.read(cx).mode() == DiffMode::Split;
    let next = if split {
        DiffMode::Unified
    } else {
        DiffMode::Split
    };
    let state = state.clone();
    Button::new(id)
        .icon(if split {
            IconName::RowsTwo
        } else {
            IconName::ColumnsTwo
        })
        .small()
        .tooltip(if split {
            TOOLTIP_UNIFIED
        } else {
            TOOLTIP_SPLIT
        })
        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.set_mode(next, cx)))
}

pub(in crate::environment_shell) fn shell_element(
    shell: &WeakEntity<EnvironmentShell>,
    cx: &mut App,
    render: impl FnOnce(&mut EnvironmentShell, &mut Context<EnvironmentShell>) -> Option<AnyElement>,
) -> AnyElement {
    shell
        .update(cx, render)
        .ok()
        .flatten()
        .unwrap_or_else(|| div().into_any_element())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn with_languages(
        &self,
        files: Vec<DiffFile>,
    ) -> Vec<DiffFile> {
        let catalog = self.language_catalog();
        files
            .into_iter()
            .map(|file| {
                let grammar = catalog
                    .for_path(Path::new(file.path().as_str()))
                    .and_then(editor_id)
                    .map(str::to_owned);
                match grammar {
                    Some(grammar) => file.with_language(grammar),
                    None => file,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_shell::review_view::numbered_lines;

    const DIFF: &str = "@@ -1,5 +1,5 @@\n keep\n-a\n-b\n+c\n mid\n-d\n+e\n+f\n";

    #[test]
    fn each_run_of_added_or_removed_lines_is_one_change_run() {
        let runs = change_runs(&numbered_lines(DIFF));
        assert_eq!(
            runs,
            [
                ChangeRun {
                    kind: LineKind::Removed,
                    first: 2,
                    last: 3
                },
                ChangeRun {
                    kind: LineKind::Added,
                    first: 2,
                    last: 2
                },
                ChangeRun {
                    kind: LineKind::Removed,
                    first: 5,
                    last: 5
                },
                ChangeRun {
                    kind: LineKind::Added,
                    first: 4,
                    last: 5
                },
            ]
        );
    }

    #[test]
    fn a_line_and_its_position_map_both_ways() {
        let lines = numbered_lines(DIFF);
        let path = SharedString::from("src/lib.rs");
        let positions: Vec<Option<(DiffSide, usize)>> = lines
            .iter()
            .map(|line| line_position(&path, line).map(|at| (at.side(), at.line())))
            .collect();
        assert_eq!(positions[0], None, "the hunk header has no line");
        assert_eq!(positions[1], Some((DiffSide::Modified, 1)));
        assert_eq!(positions[2], Some((DiffSide::Original, 2)));
        assert_eq!(positions[4], Some((DiffSide::Modified, 2)));
        assert_eq!(line_index(&lines, DiffSide::Original, 2), Some(2));
        assert_eq!(line_index(&lines, DiffSide::Modified, 2), Some(4));
        assert_eq!(line_index(&lines, DiffSide::Original, 4), Some(5));
        assert_eq!(line_index(&lines, DiffSide::Modified, 99), None);
    }

    #[test]
    fn hunks_with_headers_parse_into_one_file_at_the_new_path(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let files = parse_files(&with_headers("src/old.rs", "src/new.rs", DIFF))?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path().as_str(), "src/new.rs");
        assert_eq!((files[0].additions(), files[0].deletions()), (3, 3));
        Ok(())
    }

    #[test]
    fn an_id_keeps_its_number() {
        assert_eq!(id_number(&numbered_id("thread", 7)), Some(7));
        assert_eq!(id_number(&ElementId::Integer(7)), None);
    }
}
