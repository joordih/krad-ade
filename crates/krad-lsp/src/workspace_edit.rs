use lsp_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, Position, ResourceOp, TextDocumentEdit,
    TextEdit, WorkspaceEdit,
};
use std::path::{Path, PathBuf};

use crate::uri::uri_path;

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(index, _)| index + 1))
        .collect()
}

fn byte_offset(text: &str, starts: &[usize], at: Position) -> usize {
    let Some(&line_start) = starts.get(at.line as usize) else {
        return text.len();
    };
    let line_end = starts
        .get(at.line as usize + 1)
        .map_or(text.len(), |next| next.saturating_sub(1));
    let line = &text[line_start..line_end];
    let mut seen = 0;
    for (index, character) in line.char_indices() {
        if seen >= at.character {
            return line_start + index;
        }
        seen += u32::try_from(character.len_utf16()).unwrap_or(u32::MAX);
    }
    line_end
}

#[must_use]
pub fn descending(mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits
        .sort_by_key(|edit| std::cmp::Reverse((edit.range.start.line, edit.range.start.character)));
    edits
}

#[must_use]
pub fn apply_text_edits(text: &str, edits: Vec<TextEdit>) -> String {
    let mut edited = text.to_owned();
    for edit in descending(edits) {
        let starts = line_starts(&edited);
        let start = byte_offset(&edited, &starts, edit.range.start);
        let end = byte_offset(&edited, &starts, edit.range.end).max(start);
        edited.replace_range(start..end, &edit.new_text);
    }
    edited
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct WorkspaceChanges {
    pub created: Vec<PathBuf>,
    pub edited: Vec<(PathBuf, Vec<TextEdit>)>,
    pub renamed: Vec<(PathBuf, PathBuf)>,
    pub deleted: Vec<PathBuf>,
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

    #[must_use]
    pub fn renamed_to(&self, path: &Path) -> Option<&Path> {
        self.renamed
            .iter()
            .find(|(from, _)| from == path)
            .map(|(_, to)| to.as_path())
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

#[must_use]
pub fn workspace_changes(edit: WorkspaceEdit) -> WorkspaceChanges {
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

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Range;

    fn edit(line: u32, from: u32, to: u32, text: &str) -> TextEdit {
        TextEdit {
            range: Range {
                start: Position {
                    line,
                    character: from,
                },
                end: Position {
                    line,
                    character: to,
                },
            },
            new_text: text.to_owned(),
        }
    }

    #[test]
    fn edits_land_on_utf16_columns_from_the_last_one_back() {
        let text = "let 😀 = a;\nlet b = a;\n";
        let edited = apply_text_edits(text, vec![edit(0, 9, 10, "x"), edit(1, 8, 9, "x")]);
        assert_eq!(edited, "let 😀 = x;\nlet b = x;\n");
    }

    #[test]
    fn an_edit_past_the_end_appends() {
        assert_eq!(apply_text_edits("a", vec![edit(3, 0, 0, "b")]), "ab");
    }
}
