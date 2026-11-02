use gpui_kit::component::input::{EditorState, Rope, RopeExt as _};
use gpui_kit::{AppContext as _, Context, Entity, Window};
use krad_lsp::lsp_types::{DocumentChanges, OneOf, TextEdit, WorkspaceEdit};
use krad_lsp::uri_path;
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;

const WRITE_FAILED_TEXT: &str = "Couldn't apply the change to";

pub(super) fn descending(mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits
        .sort_by_key(|edit| std::cmp::Reverse((edit.range.start.line, edit.range.start.character)));
    edits
}

pub(super) fn apply_text_edits(text: &str, edits: Vec<TextEdit>) -> String {
    let mut rope = Rope::from(text);
    for edit in descending(edits) {
        let start = rope.position_to_offset(&edit.range.start);
        let end = rope.position_to_offset(&edit.range.end);
        rope.replace(start..end, &edit.new_text);
    }
    rope.to_string()
}

pub(super) fn file_changes(edit: WorkspaceEdit) -> Vec<(PathBuf, Vec<TextEdit>)> {
    let mut found: Vec<(PathBuf, Vec<TextEdit>)> = Vec::new();
    let mut add = |path: Option<PathBuf>, edits: Vec<TextEdit>| {
        let Some(path) = path else {
            return;
        };
        match found.iter_mut().find(|(known, _)| *known == path) {
            Some((_, known)) => known.extend(edits),
            None => found.push((path, edits)),
        }
    };
    for (uri, edits) in edit.changes.unwrap_or_default() {
        add(uri_path(&uri), edits);
    }
    let documents = match edit.document_changes {
        Some(DocumentChanges::Edits(edits)) => edits,
        Some(DocumentChanges::Operations(operations)) => operations
            .into_iter()
            .filter_map(|operation| match operation {
                krad_lsp::lsp_types::DocumentChangeOperation::Edit(edit) => Some(edit),
                krad_lsp::lsp_types::DocumentChangeOperation::Op(_) => None,
            })
            .collect(),
        None => Vec::new(),
    };
    for document in documents {
        let edits = document
            .edits
            .into_iter()
            .map(|edit| match edit {
                OneOf::Left(edit) => edit,
                OneOf::Right(annotated) => annotated.text_edit,
            })
            .collect();
        add(uri_path(&document.text_document.uri), edits);
    }
    found
}

impl EnvironmentShell {
    fn open_state(&self, path: &Path) -> Option<Entity<EditorState>> {
        self.envs
            .values()
            .flat_map(|live| live.editors.values())
            .find(|editor| editor.path == path)
            .and_then(|editor| editor.state.clone())
    }

    fn apply_editor_edits(
        state: &Entity<EditorState>,
        edits: Vec<TextEdit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let edits = descending(edits);
        state.update(cx, |state, cx| state.apply_lsp_edits(&edits, window, cx));
    }

    pub(in crate::environment_shell) fn queue_workspace_edit(
        &mut self,
        edit: WorkspaceEdit,
        cx: &mut Context<Self>,
    ) {
        self.language_servers.pending_edits.push(edit);
        cx.notify();
    }

    pub(in crate::environment_shell) fn flush_workspace_edits(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for edit in std::mem::take(&mut self.language_servers.pending_edits) {
            self.apply_workspace_edit(edit, window, cx);
        }
    }

    fn apply_workspace_edit(
        &mut self,
        edit: WorkspaceEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (path, edits) in file_changes(edit) {
            if let Some(state) = self.open_state(&path) {
                Self::apply_editor_edits(&state, edits, window, cx);
                continue;
            }
            let written = path.clone();
            let writing = cx.background_spawn(async move {
                let text = std::fs::read_to_string(&written)?;
                std::fs::write(&written, apply_text_edits(&text, edits))
            });
            cx.spawn(async move |this, cx| {
                if let Err(error) = writing.await {
                    this.update(cx, |shell, cx| {
                        shell.toast(
                            &format!("{WRITE_FAILED_TEXT} {}: {error}", path.display()),
                            cx,
                        );
                    })
                    .unwrap_or_default();
                }
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::lsp_types::{Position, Range, Uri};
    use std::collections::HashMap;
    use std::str::FromStr;

    fn replace(line: u32, from: u32, to: u32, text: &str) -> TextEdit {
        TextEdit::new(
            Range::new(Position::new(line, from), Position::new(line, to)),
            text.to_owned(),
        )
    }

    #[test]
    fn edits_apply_against_the_original_text_whatever_their_order() {
        let text = "let a = a + a;";
        let edits = vec![replace(0, 4, 5, "total"), replace(0, 12, 13, "total")];
        assert_eq!(apply_text_edits(text, edits), "let total = a + total;");
    }

    #[test]
    fn changes_of_one_file_gather_from_both_shapes() {
        let Ok(uri) = Uri::from_str("file:///work/lib.rs") else {
            unreachable!("a literal file URI parses");
        };
        let edit = WorkspaceEdit {
            changes: Some(HashMap::from([(uri, vec![replace(0, 0, 1, "x")])])),
            ..WorkspaceEdit::default()
        };
        let found = file_changes(edit);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, PathBuf::from("/work/lib.rs"));
        assert_eq!(found[0].1.len(), 1);
    }
}
