use krad_domain::agents_api::{CodeLine, CodePosition, CodeRequest, CodeTool};
use lsp_types::request::{
    DocumentDiagnosticRequest, GotoDefinition, HoverRequest, References, Rename,
};
use lsp_types::{
    DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, GotoDefinitionParams, GotoDefinitionResponse, HoverContents,
    HoverParams, Location, MarkedString, PartialResultParams, Position, ReferenceContext,
    ReferenceParams, RenameParams, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams, WorkspaceEdit,
};
use std::path::{Path, PathBuf};

use crate::client::LspClient;
use crate::error::LspError;
use crate::uri::{file_uri, uri_path};
use crate::workspace_edit::workspace_changes;

const RENAMED: &str = "renamed";

fn line(path: &Path, root: &Path, at: Position, text: String) -> CodeLine {
    CodeLine {
        path: Some(
            path.strip_prefix(root)
                .unwrap_or(path)
                .display()
                .to_string(),
        ),
        position: Some(CodePosition {
            line: at.line,
            character: at.character,
        }),
        text,
    }
}

fn locations(found: Vec<Location>, root: &Path) -> Vec<CodeLine> {
    found
        .into_iter()
        .filter_map(|location| {
            let path = uri_path(&location.uri)?;
            Some(line(&path, root, location.range.start, String::new()))
        })
        .collect()
}

fn hover_text(contents: HoverContents) -> String {
    let marked = |item: MarkedString| match item {
        MarkedString::String(text) => text,
        MarkedString::LanguageString(code) => code.value,
    };
    match contents {
        HoverContents::Scalar(item) => marked(item),
        HoverContents::Array(items) => items.into_iter().map(marked).collect::<Vec<_>>().join("\n"),
        HoverContents::Markup(markup) => markup.value,
    }
}

fn severity(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        _ => "hint",
    }
}

#[must_use]
pub fn confined(edit: &WorkspaceEdit, root: &Path) -> bool {
    let changes = workspace_changes(edit.clone());
    let touched = changes
        .edited
        .iter()
        .map(|(path, _)| path)
        .chain(changes.created.iter())
        .chain(changes.deleted.iter())
        .chain(changes.renamed.iter().flat_map(|(from, to)| [from, to]))
        .all(|path| path.starts_with(root));
    touched
}

pub type Answered = (Vec<CodeLine>, Option<WorkspaceEdit>);

/// # Errors
///
/// Fails when the language server refuses or cannot answer the request.
pub async fn ask_code(
    client: LspClient,
    request: CodeRequest,
    path: PathBuf,
    root: PathBuf,
) -> Result<Answered, LspError> {
    let document = TextDocumentIdentifier::new(file_uri(&path)?);
    let at = request
        .position
        .map_or_else(Position::default, |at| Position::new(at.line, at.character));
    let position = TextDocumentPositionParams::new(document.clone(), at);
    match request.tool {
        CodeTool::Diagnostics => diagnostics(&client, document, &path, &root).await,
        CodeTool::Definition => definition(&client, position, &root).await,
        CodeTool::References => references(&client, position, &root).await,
        CodeTool::Hover => hover(&client, position).await,
        CodeTool::Rename => {
            rename(
                &client,
                position,
                request.new_name.unwrap_or_default(),
                &root,
            )
            .await
        }
    }
}

async fn diagnostics(
    client: &LspClient,
    document: TextDocumentIdentifier,
    path: &Path,
    root: &Path,
) -> Result<Answered, LspError> {
    let report = client
        .request::<DocumentDiagnosticRequest>(&DocumentDiagnosticParams {
            text_document: document,
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await?;
    let items = match report {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(full)) => {
            full.full_document_diagnostic_report.items
        }
        _ => Vec::new(),
    };
    let lines = items
        .into_iter()
        .map(|diagnostic| {
            line(
                path,
                root,
                diagnostic.range.start,
                format!("{}: {}", severity(diagnostic.severity), diagnostic.message),
            )
        })
        .collect();
    Ok((lines, None))
}

async fn definition(
    client: &LspClient,
    position: TextDocumentPositionParams,
    root: &Path,
) -> Result<Answered, LspError> {
    let found = client
        .request::<GotoDefinition>(&GotoDefinitionParams {
            text_document_position_params: position,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .await?;
    let found = match found {
        Some(GotoDefinitionResponse::Scalar(location)) => vec![location],
        Some(GotoDefinitionResponse::Array(found)) => found,
        Some(GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|link| Location::new(link.target_uri, link.target_selection_range))
            .collect(),
        None => Vec::new(),
    };
    Ok((locations(found, root), None))
}

async fn references(
    client: &LspClient,
    position: TextDocumentPositionParams,
    root: &Path,
) -> Result<Answered, LspError> {
    let found = client
        .request::<References>(&ReferenceParams {
            text_document_position: position,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext {
                include_declaration: false,
            },
        })
        .await?
        .unwrap_or_default();
    Ok((locations(found, root), None))
}

async fn hover(
    client: &LspClient,
    position: TextDocumentPositionParams,
) -> Result<Answered, LspError> {
    let hover = client
        .request::<HoverRequest>(&HoverParams {
            text_document_position_params: position,
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await?;
    let text = hover.map_or_else(String::new, |hover| hover_text(hover.contents));
    let lines = if text.is_empty() {
        Vec::new()
    } else {
        vec![CodeLine {
            path: None,
            position: None,
            text,
        }]
    };
    Ok((lines, None))
}

async fn rename(
    client: &LspClient,
    position: TextDocumentPositionParams,
    new_name: String,
    root: &Path,
) -> Result<Answered, LspError> {
    let edit = client
        .request::<Rename>(&RenameParams {
            text_document_position: position,
            new_name,
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .await?;
    let Some(edit) = edit else {
        return Ok((Vec::new(), None));
    };
    let changes = workspace_changes(edit.clone());
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let lines = changes
        .edited
        .iter()
        .map(|(file, edits)| CodeLine {
            path: Some(relative(file)),
            position: None,
            text: format!("{} edits", edits.len()),
        })
        .chain(changes.renamed.iter().map(|(from, to)| CodeLine {
            path: Some(relative(from)),
            position: None,
            text: format!("{RENAMED} to {}", relative(to)),
        }))
        .collect();
    Ok((lines, Some(edit)))
}
