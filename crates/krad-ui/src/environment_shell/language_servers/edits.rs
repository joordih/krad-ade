use gpui_kit::component::input::{EditorState, Rope, RopeExt as _};
use gpui_kit::{AppContext as _, Context, Entity, Window};
use krad_host::HostLink;
use krad_lsp::lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, ResourceOp, TextDocumentEdit, TextEdit,
    WorkspaceEdit,
};
use krad_lsp::uri_path;
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::Document;
use krad_domain::Host;

const WRITE_FAILED_TEXT: &str = "Couldn't apply the change to";
const MOVE_UNSUPPORTED_TEXT: &str = "The language server asked to move a file to another folder:";

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

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct WorkspaceChanges {
    pub(super) created: Vec<PathBuf>,
    pub(super) edited: Vec<(PathBuf, Vec<TextEdit>)>,
    pub(super) renamed: Vec<(PathBuf, PathBuf)>,
    pub(super) deleted: Vec<PathBuf>,
}

impl WorkspaceChanges {
    fn edit(&mut self, path: Option<PathBuf>, edits: Vec<TextEdit>) {
        let Some(path) = path else {
            return;
        };
        match self.edited.iter_mut().find(|(known, _)| *known == path) {
            Some((_, known)) => known.extend(edits),
            None => self.edited.push((path, edits)),
        }
    }

    fn operation(&mut self, operation: ResourceOp) {
        match operation {
            ResourceOp::Create(create) => self.created.extend(uri_path(&create.uri)),
            ResourceOp::Rename(rename) => {
                if let (Some(from), Some(to)) =
                    (uri_path(&rename.old_uri), uri_path(&rename.new_uri))
                {
                    self.renamed.push((from, to));
                }
            }
            ResourceOp::Delete(delete) => self.deleted.extend(uri_path(&delete.uri)),
        }
    }

    fn renamed_to(&self, path: &Path) -> Option<&Path> {
        self.renamed
            .iter()
            .find(|(from, _)| from == path)
            .map(|(_, to)| to.as_path())
    }
}

fn document_edits(document: TextDocumentEdit) -> (Option<PathBuf>, Vec<TextEdit>) {
    let edits = document
        .edits
        .into_iter()
        .map(|edit| match edit {
            OneOf::Left(edit) => edit,
            OneOf::Right(annotated) => annotated.text_edit,
        })
        .collect();
    (uri_path(&document.text_document.uri), edits)
}

pub(super) fn workspace_changes(edit: WorkspaceEdit) -> WorkspaceChanges {
    let mut changes = WorkspaceChanges::default();
    for (uri, edits) in edit.changes.unwrap_or_default() {
        changes.edit(uri_path(&uri), edits);
    }
    match edit.document_changes {
        Some(DocumentChanges::Edits(documents)) => {
            for document in documents {
                let (path, edits) = document_edits(document);
                changes.edit(path, edits);
            }
        }
        Some(DocumentChanges::Operations(operations)) => {
            for operation in operations {
                match operation {
                    DocumentChangeOperation::Edit(document) => {
                        let (path, edits) = document_edits(document);
                        changes.edit(path, edits);
                    }
                    DocumentChangeOperation::Op(operation) => changes.operation(operation),
                }
            }
        }
        None => {}
    }
    changes
}

impl EnvironmentShell {
    pub(super) fn open_state(&self, document: &Document) -> Option<Entity<EditorState>> {
        self.envs
            .values()
            .filter(|live| live.data.host == document.host)
            .flat_map(|live| live.editors.values())
            .find(|editor| editor.path == document.path)
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
        host: Host,
        edit: WorkspaceEdit,
        cx: &mut Context<Self>,
    ) {
        self.language_servers.pending_edits.push((host, edit));
        cx.notify();
    }

    pub(in crate::environment_shell) fn flush_workspace_edits(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (host, edit) in std::mem::take(&mut self.language_servers.pending_edits) {
            self.apply_workspace_edit(&host, edit, window, cx);
        }
        if let Some((host, path, at)) = self.language_servers.pending_rename.take() {
            self.start_rename_at(&Document::new(&host, &path), at, window, cx);
        }
        self.show_pending_choice(window, cx);
    }

    fn apply_workspace_edit(
        &mut self,
        host: &Host,
        edit: WorkspaceEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changes = workspace_changes(edit);
        let link = self.server_link(host).map(|(link, _)| link);
        let mut renamed_after_write = Vec::new();
        for (path, edits) in &changes.edited {
            if let Some(state) = self.open_state(&Document::new(host, path)) {
                Self::apply_editor_edits(&state, edits.clone(), window, cx);
                continue;
            }
            let Some(link) = link.clone() else {
                self.toast(&format!("{WRITE_FAILED_TEXT} {}", path.display()), cx);
                continue;
            };
            let rename = changes.renamed_to(path).map(Path::to_path_buf);
            if rename.is_some() {
                renamed_after_write.push(path.clone());
            }
            Self::write_edited(
                link,
                path.clone(),
                edits.clone(),
                changes.created.contains(path),
                rename,
                cx,
            );
        }
        for (from, to) in &changes.renamed {
            if !renamed_after_write.contains(from) {
                self.rename_to(from, to, cx);
            }
        }
        for path in changes.deleted {
            self.delete_file(path, cx);
        }
    }

    fn write_edited(
        link: HostLink,
        path: PathBuf,
        edits: Vec<TextEdit>,
        created: bool,
        rename: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let written = path.clone();
        let writing = cx.background_spawn(async move {
            let text = if created && !link.exists(&written) {
                String::new()
            } else {
                link.read_to_string(&written)?
            };
            link.write(&written, apply_text_edits(&text, edits).as_bytes())
        });
        cx.spawn(async move |this, cx| {
            let written = writing.await;
            this.update(cx, |shell, cx| match written {
                Ok(()) => {
                    if let Some(to) = rename {
                        shell.rename_to(&path, &to, cx);
                    }
                }
                Err(error) => shell.toast(
                    &format!("{WRITE_FAILED_TEXT} {}: {error}", path.display()),
                    cx,
                ),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn rename_to(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        let (Some(name), true) = (to.file_name(), from.parent() == to.parent()) else {
            self.toast(&format!("{MOVE_UNSUPPORTED_TEXT} {}", from.display()), cx);
            return;
        };
        self.rename_file(from.to_path_buf(), &name.to_string_lossy(), cx);
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
        let found = workspace_changes(edit).edited;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, PathBuf::from("/work/lib.rs"));
        assert_eq!(found[0].1.len(), 1);
    }

    #[test]
    fn file_operations_keep_apart_from_text_edits() {
        let uri = |path: &str| {
            Uri::from_str(&format!("file://{path}"))
                .unwrap_or_else(|_| unreachable!("a literal file URI parses"))
        };
        let edit = WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(vec![
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: krad_lsp::lsp_types::OptionalVersionedTextDocumentIdentifier {
                        uri: uri("/work/Names.java"),
                        version: None,
                    },
                    edits: vec![OneOf::Left(replace(0, 0, 5, "Labels"))],
                }),
                DocumentChangeOperation::Op(ResourceOp::Rename(krad_lsp::lsp_types::RenameFile {
                    old_uri: uri("/work/Names.java"),
                    new_uri: uri("/work/Labels.java"),
                    options: None,
                    annotation_id: None,
                })),
            ])),
            ..WorkspaceEdit::default()
        };
        let changes = workspace_changes(edit);
        assert_eq!(changes.edited.len(), 1);
        assert_eq!(
            changes.renamed,
            vec![(
                PathBuf::from("/work/Names.java"),
                PathBuf::from("/work/Labels.java")
            )]
        );
        assert_eq!(
            changes.renamed_to(Path::new("/work/Names.java")),
            Some(Path::new("/work/Labels.java"))
        );
    }
}
