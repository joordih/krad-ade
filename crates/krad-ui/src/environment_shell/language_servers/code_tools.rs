use futures::channel::oneshot::Sender as ReplySender;
use gpui_kit::{AppContext as _, Context};
use krad_agents::{CodeAnswer, CodeLine, CodePosition, CodeRequest, CodeTool, ErrorCode, Response};
use krad_domain::{EnvironmentName, GroupId, TerminalId};
use krad_lsp::lsp_types::request::{
    DocumentDiagnosticRequest, GotoDefinition, HoverRequest, References, Rename,
};
use krad_lsp::lsp_types::{
    DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, GotoDefinitionParams, GotoDefinitionResponse, HoverContents,
    HoverParams, Location, MarkedString, PartialResultParams, Position, ReferenceContext,
    ReferenceParams, RenameParams, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams, WorkspaceEdit,
};
use krad_lsp::{file_uri, uri_path, LspClient, LspError};
use std::path::{Path, PathBuf};

use super::super::EnvironmentShell;
use super::edits::workspace_changes;
use super::{Document, ServerKey, ServerRoot};

const NO_SERVER: &str =
    "No language server runs for this file yet: open a file of its language in the editor";
const NO_GROUP: &str = "This agent has no group folder";
const OUTSIDE_GROUP: &str = "The change would touch files outside the agent's group folder";
const NOT_A_FILE: &str = "The path is not a file of the group";
const RENAMED: &str = "renamed";

pub(in crate::environment_shell) enum CodeCaller {
    Terminal(TerminalId),
    Group(GroupId),
}

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

fn confined(edit: &WorkspaceEdit, root: &Path) -> bool {
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

type Answered = (Vec<CodeLine>, Option<WorkspaceEdit>);

async fn ask(
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

impl EnvironmentShell {
    pub(in crate::environment_shell) fn answer_code(
        &mut self,
        environment: &str,
        caller: &CodeCaller,
        request: CodeRequest,
        reply: ReplySender<Response>,
        cx: &mut Context<Self>,
    ) {
        let env = EnvironmentName::new(environment.to_owned());
        let Some(live) = self.envs.get(&env) else {
            reply
                .send(Response::error(ErrorCode::Unsupported, NO_GROUP))
                .unwrap_or_default();
            return;
        };
        let group = match caller {
            CodeCaller::Terminal(id) => live.data.group_of(*id),
            CodeCaller::Group(id) => live.data.group(*id),
        };
        let Some(root) = group.map(|group| group.root.clone()) else {
            reply
                .send(Response::error(ErrorCode::Unsupported, NO_GROUP))
                .unwrap_or_default();
            return;
        };
        let host = live.data.host.clone();
        let requested = PathBuf::from(&request.path);
        let path = if requested.is_absolute() {
            requested
        } else {
            root.join(requested)
        };
        if !path.starts_with(&root) {
            reply
                .send(Response::error(ErrorCode::NotFound, NOT_A_FILE))
                .unwrap_or_default();
            return;
        }
        let document = Document::new(&host, &path);
        let Some((key, client)) = self.code_server(&document) else {
            reply
                .send(Response::error(ErrorCode::Unsupported, NO_SERVER))
                .unwrap_or_default();
            return;
        };
        let partial = self
            .language_servers
            .running(&key)
            .is_some_and(|running| running.progress.label().is_some());
        let modifies = request.tool.modifies();
        let asked = cx.background_spawn(ask(client, request, path, root.clone()));
        cx.spawn(async move |this, cx| {
            let answered = asked.await;
            this.update(cx, |shell, cx| {
                let response = match answered {
                    Ok((_, Some(edit))) if modifies && !confined(&edit, &root) => {
                        Response::error(ErrorCode::Forbidden, OUTSIDE_GROUP)
                    }
                    Ok((lines, edit)) => {
                        if let Some(edit) = edit {
                            shell.queue_workspace_edit(host, edit, cx);
                        }
                        Response::Code(CodeAnswer { lines, partial })
                    }
                    Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
                };
                reply.send(response).unwrap_or_default();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn code_server(&self, document: &Document) -> Option<(ServerKey, LspClient)> {
        if let Some(key) = self.language_servers.documents.get(document) {
            let running = self.language_servers.running(key)?;
            return Some((key.clone(), running.server.client.clone()));
        }
        let language = self.language_servers.catalog.for_path(&document.path)?;
        let plan = self.language_servers.tools.as_ref()?.plan(language)?;
        self.language_servers.slots.keys().find_map(|key| {
            let serves = key.host == document.host
                && key.plan.package == plan.package
                && match &key.root {
                    ServerRoot::Host => true,
                    ServerRoot::Folder(folder) => document.path.starts_with(folder),
                };
            let running = self.language_servers.running(key).filter(|_| serves)?;
            Some((key.clone(), running.server.client.clone()))
        })
    }
}
