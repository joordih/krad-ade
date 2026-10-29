use gpui_kit::component::input::Position;
use gpui_kit::{AppContext as _, Context, Task, Window};
use krad_domain::{EnvironmentView, FileOpening, VaultIndex};
use krad_repo::{search_text, TextMatch};
use std::path::Path;

use super::super::EnvironmentShell;

const NOTES_FOUND: usize = 8;
const LINES_FOUND: usize = 20;
const MIN_QUERY_CHARS: usize = 2;
const PATH_SEPARATOR: char = '/';

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct NoteLine {
    pub(in crate::environment_shell) path: String,
    pub(in crate::environment_shell) line: u32,
    pub(in crate::environment_shell) text: String,
}

#[derive(Default)]
pub(in crate::environment_shell) struct NotesSearch {
    query: String,
    lines: Vec<NoteLine>,
    task: Option<Task<()>>,
    pending: Option<(String, u32)>,
}

fn long_enough(query: &str) -> bool {
    query.trim().chars().count() >= MIN_QUERY_CHARS
}

fn note_lines(vault: &Path, index: &VaultIndex, matches: Vec<TextMatch>) -> Vec<NoteLine> {
    let mut lines: Vec<NoteLine> = matches
        .into_iter()
        .filter_map(|found| {
            let path = found
                .path
                .strip_prefix(vault)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            index.note(&path)?;
            Some(NoteLine {
                path,
                line: found.line,
                text: found.text,
            })
        })
        .collect();
    let mut order: Vec<String> = Vec::new();
    for line in &lines {
        if !order.contains(&line.path) {
            order.push(line.path.clone());
        }
    }
    lines.sort_by_key(|line| (order.iter().position(|path| *path == line.path), line.line));
    lines
}

pub(in crate::environment_shell) fn note_line_label(line: &NoteLine) -> String {
    let name = line
        .path
        .rsplit(PATH_SEPARATOR)
        .next()
        .unwrap_or(&line.path);
    let name = name
        .strip_suffix(krad_domain::NOTE_EXTENSION)
        .and_then(|stem| stem.strip_suffix('.'))
        .unwrap_or(name);
    format!("{name}:{} {}", line.line, line.text)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn palette_notes(&self, query: &str) -> Vec<String> {
        if !long_enough(query) {
            return Vec::new();
        }
        self.notes
            .index
            .find(query, NOTES_FOUND)
            .into_iter()
            .map(|note| note.path.clone())
            .collect()
    }

    pub(in crate::environment_shell) fn palette_note_lines(&self) -> &[NoteLine] {
        &self.notes.search.lines
    }

    pub(in crate::environment_shell) fn search_notes(
        &mut self,
        query: &str,
        cx: &mut Context<Self>,
    ) {
        self.sync_vault(cx);
        let query = query.trim().to_owned();
        if query == self.notes.search.query {
            return;
        }
        self.notes.search.query.clone_from(&query);
        let vault = self.vault_file("");
        let (Some(vault), true) = (vault, long_enough(&query)) else {
            self.notes.search.lines.clear();
            self.notes.search.task = None;
            cx.notify();
            return;
        };
        let searched = vault.clone();
        let search =
            cx.background_spawn(async move { search_text(&searched, &query, LINES_FOUND) });
        self.notes.search.task = Some(cx.spawn(async move |this, cx| {
            let found = search.await;
            this.update(cx, |shell, cx| {
                match found {
                    Ok(matches) => {
                        shell.notes.search.lines = note_lines(&vault, &shell.notes.index, matches);
                    }
                    Err(error) => shell.push_toast(error.to_string()),
                }
                cx.notify();
            })
            .unwrap_or_default();
        }));
    }

    pub(in crate::environment_shell) fn open_note_from_palette(
        &mut self,
        path: &str,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        self.set_view(EnvironmentView::Notes, cx);
        self.notes.graph.showing = false;
        self.open_vault_note(path, FileOpening::Preview, cx);
        self.notes.search.pending = line.map(|line| (path.to_owned(), line));
    }

    pub(super) fn place_pending_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, line)) = self.notes.search.pending.clone() else {
            return;
        };
        let Some(state) = self
            .notes
            .editors
            .get(&path)
            .filter(|editor| editor.text(cx).is_some())
            .and_then(|editor| editor.state.clone())
        else {
            return;
        };
        self.notes.search.pending = None;
        state.update(cx, |state, cx| {
            state.set_cursor_position(Position::new(line.saturating_sub(1), 0), window, cx);
            state.focus(window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::Note;
    use std::path::PathBuf;

    fn note(path: &str) -> (String, Note) {
        (path.to_owned(), Note::parse("# Title\n"))
    }

    fn found(path: &str, line: u32) -> TextMatch {
        TextMatch {
            path: PathBuf::from("/repo/docs/notes").join(path),
            line,
            text: String::from("text"),
        }
    }

    #[test]
    fn lines_stay_inside_the_vault_and_group_by_note() {
        let index = VaultIndex::new(vec![note("A.md"), note("Ideas/B.md")]);
        let matches = vec![
            found("A.md", 3),
            found("Ideas/B.md", 1),
            found("A.md", 9),
            found("attachments/log.txt", 2),
            TextMatch {
                path: PathBuf::from("/elsewhere/A.md"),
                line: 1,
                text: String::new(),
            },
        ];
        let lines = note_lines(Path::new("/repo/docs/notes"), &index, matches);
        let places: Vec<(&str, u32)> = lines
            .iter()
            .map(|line| (line.path.as_str(), line.line))
            .collect();
        assert_eq!(places, [("A.md", 3), ("A.md", 9), ("Ideas/B.md", 1)]);
        assert_eq!(note_line_label(&lines[2]), "B:1 text");
    }

    #[test]
    fn short_queries_find_nothing() {
        assert!(!long_enough(" a "));
        assert!(long_enough("ab"));
    }
}
