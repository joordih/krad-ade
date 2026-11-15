use krad_lsp::lsp_types::{
    self, CompletionResponse, CompletionTextEdit, DocumentChanges, Documentation,
    GotoDefinitionResponse, HoverContents, MarkedString, OneOf, PublishDiagnosticsParams,
    SignatureHelp, WorkspaceEdit,
};
use krad_lsp::uri_path;
use krad_protocol::{
    CodeDiagnostic, CodeLocation, Completion, DiagnosticSeverity, FileEdits, RangeEdit, Signature,
    TextPosition, TextRange,
};
use std::path::PathBuf;

const SECTION_BREAK: &str = "\n\n";

pub(crate) const fn position(at: TextPosition) -> lsp_types::Position {
    lsp_types::Position {
        line: at.line,
        character: at.character,
    }
}

const fn shown_position(at: lsp_types::Position) -> TextPosition {
    TextPosition {
        line: at.line,
        character: at.character,
    }
}

pub(crate) const fn range(range: lsp_types::Range) -> TextRange {
    TextRange {
        start: shown_position(range.start),
        end: shown_position(range.end),
    }
}

fn documentation(documentation: Option<Documentation>) -> Option<String> {
    documentation.map(|documentation| match documentation {
        Documentation::String(text) => text,
        Documentation::MarkupContent(markup) => markup.value,
    })
}

fn completion(item: lsp_types::CompletionItem) -> Completion {
    let (insert, replace) = match item.text_edit {
        Some(CompletionTextEdit::Edit(edit)) => (edit.new_text, Some(range(edit.range))),
        Some(CompletionTextEdit::InsertAndReplace(edit)) => {
            (edit.new_text, Some(range(edit.replace)))
        }
        None => (item.insert_text.unwrap_or_else(|| item.label.clone()), None),
    };
    let kind = item
        .kind
        .and_then(|kind| serde_json::to_value(kind).ok())
        .and_then(|value| value.as_u64())
        .and_then(|kind| u32::try_from(kind).ok());
    Completion {
        label: item.label,
        detail: item.detail,
        documentation: documentation(item.documentation),
        insert,
        replace,
        kind,
    }
}

pub(crate) fn completions(response: Option<CompletionResponse>) -> Vec<Completion> {
    match response {
        Some(CompletionResponse::Array(items)) => items.into_iter().map(completion).collect(),
        Some(CompletionResponse::List(list)) => list.items.into_iter().map(completion).collect(),
        None => Vec::new(),
    }
}

fn marked(marked: MarkedString) -> String {
    match marked {
        MarkedString::String(text) => text,
        MarkedString::LanguageString(code) => format!("```{}\n{}\n```", code.language, code.value),
    }
}

pub(crate) fn hover(hover: Option<lsp_types::Hover>) -> Option<String> {
    let text = match hover?.contents {
        HoverContents::Scalar(scalar) => marked(scalar),
        HoverContents::Array(parts) => parts
            .into_iter()
            .map(marked)
            .collect::<Vec<_>>()
            .join(SECTION_BREAK),
        HoverContents::Markup(markup) => markup.value,
    };
    (!text.trim().is_empty()).then_some(text)
}

fn location(location: &lsp_types::Location) -> Option<CodeLocation> {
    Some(CodeLocation {
        path: uri_path(&location.uri)?,
        range: range(location.range),
    })
}

pub(crate) fn definitions(response: Option<GotoDefinitionResponse>) -> Vec<CodeLocation> {
    match response {
        Some(GotoDefinitionResponse::Scalar(found)) => location(&found).into_iter().collect(),
        Some(GotoDefinitionResponse::Array(found)) => found.iter().filter_map(location).collect(),
        Some(GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .filter_map(|link| {
                Some(CodeLocation {
                    path: uri_path(&link.target_uri)?,
                    range: range(link.target_selection_range),
                })
            })
            .collect(),
        None => Vec::new(),
    }
}

pub(crate) fn references(found: Option<Vec<lsp_types::Location>>) -> Vec<CodeLocation> {
    found
        .unwrap_or_default()
        .iter()
        .filter_map(location)
        .collect()
}

pub(crate) fn signatures(help: Option<SignatureHelp>) -> (Vec<Signature>, Option<u32>) {
    let Some(help) = help else {
        return (Vec::new(), None);
    };
    let shown = help
        .signatures
        .into_iter()
        .map(|signature| Signature {
            label: signature.label,
            documentation: documentation(signature.documentation),
            active_parameter: signature.active_parameter.or(help.active_parameter),
        })
        .collect();
    (shown, help.active_signature)
}

fn range_edits(edits: Vec<lsp_types::TextEdit>) -> Vec<RangeEdit> {
    edits
        .into_iter()
        .map(|edit| RangeEdit {
            range: range(edit.range),
            text: edit.new_text,
        })
        .collect()
}

pub(crate) fn file_edits(path: PathBuf, edits: Option<Vec<lsp_types::TextEdit>>) -> Vec<FileEdits> {
    let edits = range_edits(edits.unwrap_or_default());
    if edits.is_empty() {
        return Vec::new();
    }
    vec![FileEdits { path, edits }]
}

pub(crate) fn workspace_edits(edit: Option<WorkspaceEdit>) -> Vec<FileEdits> {
    let Some(edit) = edit else {
        return Vec::new();
    };
    let mut files: Vec<FileEdits> = edit
        .changes
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(uri, edits)| {
            Some(FileEdits {
                path: uri_path(&uri)?,
                edits: range_edits(edits),
            })
        })
        .collect();
    let documents = match edit.document_changes {
        Some(DocumentChanges::Edits(edits)) => edits,
        Some(DocumentChanges::Operations(operations)) => operations
            .into_iter()
            .filter_map(|operation| match operation {
                lsp_types::DocumentChangeOperation::Edit(edit) => Some(edit),
                lsp_types::DocumentChangeOperation::Op(_) => None,
            })
            .collect(),
        None => Vec::new(),
    };
    files.extend(documents.into_iter().filter_map(|document| {
        let edits = document
            .edits
            .into_iter()
            .map(|edit| match edit {
                OneOf::Left(edit) => edit,
                OneOf::Right(annotated) => annotated.text_edit,
            })
            .collect();
        Some(FileEdits {
            path: uri_path(&document.text_document.uri)?,
            edits: range_edits(edits),
        })
    }));
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

const fn severity(severity: Option<lsp_types::DiagnosticSeverity>) -> DiagnosticSeverity {
    match severity {
        Some(lsp_types::DiagnosticSeverity::WARNING) => DiagnosticSeverity::Warning,
        Some(lsp_types::DiagnosticSeverity::INFORMATION) => DiagnosticSeverity::Information,
        Some(lsp_types::DiagnosticSeverity::HINT) => DiagnosticSeverity::Hint,
        _ => DiagnosticSeverity::Error,
    }
}

pub(crate) fn diagnostics(
    params: PublishDiagnosticsParams,
) -> Option<(PathBuf, Vec<CodeDiagnostic>)> {
    let path = uri_path(&params.uri)?;
    let shown = params
        .diagnostics
        .into_iter()
        .map(|diagnostic| CodeDiagnostic {
            range: range(diagnostic.range),
            severity: severity(diagnostic.severity),
            message: diagnostic.message,
            source: diagnostic.source,
        })
        .collect();
    Some((path, shown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_lsp::file_uri;
    use krad_lsp::lsp_types::{CompletionItem, MarkupContent, MarkupKind, Position, Range};

    fn span(line: u32, from: u32, to: u32) -> Range {
        Range {
            start: Position {
                line,
                character: from,
            },
            end: Position {
                line,
                character: to,
            },
        }
    }

    #[test]
    fn a_completion_inserts_its_edit_or_falls_back_to_its_label() {
        let edited = CompletionItem {
            label: "println!".to_owned(),
            text_edit: Some(CompletionTextEdit::Edit(lsp_types::TextEdit {
                range: span(2, 4, 7),
                new_text: "println!(\"$0\")".to_owned(),
            })),
            ..CompletionItem::default()
        };
        let plain = CompletionItem {
            label: "len".to_owned(),
            ..CompletionItem::default()
        };
        let shown = completions(Some(CompletionResponse::Array(vec![edited, plain])));
        assert_eq!(shown[0].insert, "println!(\"$0\")");
        assert_eq!(shown[0].replace.map(|range| range.start.character), Some(4));
        assert_eq!(shown[1].insert, "len");
        assert_eq!(shown[1].replace, None);
    }

    #[test]
    fn hover_markup_is_kept_and_an_empty_hover_is_none() {
        let markup = |value: &str| lsp_types::Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: value.to_owned(),
            }),
            range: None,
        };
        assert_eq!(
            hover(Some(markup("`fn main()`"))),
            Some("`fn main()`".to_owned())
        );
        assert_eq!(hover(Some(markup("  "))), None);
    }

    #[test]
    fn a_rename_lists_the_edits_of_every_file() -> Result<(), Box<dyn std::error::Error>> {
        let main = PathBuf::from("/home/dev/app/src/main.rs");
        let lib = PathBuf::from("/home/dev/app/src/lib.rs");
        let edit = |line| lsp_types::TextEdit {
            range: span(line, 3, 6),
            new_text: "renamed".to_owned(),
        };
        let document = |path: &PathBuf, edits: Vec<lsp_types::TextEdit>| {
            Ok::<_, krad_lsp::LspError>(lsp_types::TextDocumentEdit {
                text_document: lsp_types::OptionalVersionedTextDocumentIdentifier {
                    uri: file_uri(path)?,
                    version: None,
                },
                edits: edits.into_iter().map(OneOf::Left).collect(),
            })
        };
        let files = workspace_edits(Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Edits(vec![
                document(&main, vec![edit(1)])?,
                document(&lib, vec![edit(4), edit(9)])?,
            ])),
            ..WorkspaceEdit::default()
        }));
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, lib);
        assert_eq!(files[0].edits.len(), 2);
        assert_eq!(files[1].edits[0].text, "renamed");
        Ok(())
    }

    #[test]
    fn a_diagnostic_without_severity_is_an_error() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from("/home/dev/app/src/main.rs");
        let params = PublishDiagnosticsParams {
            uri: file_uri(&path)?,
            diagnostics: vec![lsp_types::Diagnostic {
                range: span(0, 0, 2),
                message: "expected `;`".to_owned(),
                ..lsp_types::Diagnostic::default()
            }],
            version: None,
        };
        let (shown_path, shown) = diagnostics(params).ok_or("no path")?;
        assert_eq!(shown_path, path);
        assert_eq!(shown[0].severity, DiagnosticSeverity::Error);
        Ok(())
    }
}
