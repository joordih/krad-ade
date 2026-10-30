use krad_domain::{line_diff, FileEdit};
use std::path::Path;

use super::Ran;

const WROTE_TEXT: &str = "Wrote";
const BYTES_TEXT: &str = "bytes to";

pub(super) fn preview(path: &Path, shown: &str, content: &str) -> FileEdit {
    let previous = std::fs::read_to_string(path).unwrap_or_default();
    FileEdit {
        path: shown.to_owned(),
        lines: line_diff(&previous, content),
    }
}

pub(super) fn write(path: &Path, shown: &str, content: &str) -> Ran {
    let edit = preview(path, shown, content);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, content));
    match written {
        Ok(()) => Ran {
            content: format!("{WROTE_TEXT} {} {BYTES_TEXT} {shown}", content.len()),
            failed: false,
            edit: Some(edit),
        },
        Err(error) => Ran::failed(format!("{shown}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::scratch::Scratch;
    use super::*;
    use krad_domain::LineChange;

    #[test]
    fn writing_creates_the_folders_and_reports_the_change() {
        let scratch = Scratch::new();
        let path = scratch.path().join("deep/down/new.txt");
        let ran = write(&path, "deep/down/new.txt", "hello\n");
        assert!(!ran.failed, "{}", ran.content);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            "hello\n"
        );
        let added = ran.edit.map(|edit| edit.counts());
        assert_eq!(added, Some((1, 0)));
    }

    #[test]
    fn overwriting_diffs_against_the_old_content() {
        let scratch = Scratch::new();
        let path = scratch.file("a.txt", "old\n");
        let edit = preview(&path, "a.txt", "new\n");
        assert_eq!(edit.path, "a.txt");
        assert!(edit
            .lines
            .iter()
            .any(|line| line.change == LineChange::Removed && line.text == "old"));
    }
}
