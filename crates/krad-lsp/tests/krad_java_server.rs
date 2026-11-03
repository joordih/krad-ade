use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::ServerLaunch;
use krad_host::HostLink;
use krad_lsp::lsp_types::request::{
    CodeActionRequest, CodeActionResolveRequest, Completion, DocumentDiagnosticRequest,
    GotoDefinition, Rename,
};
use krad_lsp::lsp_types::{
    CodeActionContext, CodeActionOrCommand, CodeActionParams, CompletionParams, CompletionResponse,
    DocumentChangeOperation, DocumentChanges, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, GotoDefinitionParams, PartialResultParams, Position, Range,
    RenameParams, ResourceOp, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams,
};
use krad_lsp::{file_uri, DocumentChange, LanguageServer, LspClient, ServerEvent, ServerSettings};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const SERVER_VARIABLE: &str = "KRAD_JAVA_SERVER";
const SAMPLE_VARIABLE: &str = "KRAD_JAVA_SAMPLE";
const JAVA_HOME_VARIABLE: &str = "JAVA_HOME";
const SETTLE: Duration = Duration::from_secs(4);
const READY: Duration = Duration::from_mins(3);

type Outcome = Result<(), Box<dyn Error>>;

fn position(text: &str, needle: &str, offset: u32) -> Option<Position> {
    text.lines().enumerate().find_map(|(line, content)| {
        let column = content.find(needle)?;
        Some(Position::new(
            u32::try_from(line).ok()?,
            u32::try_from(column).ok()? + offset,
        ))
    })
}

fn start(root: &Path) -> Result<(LanguageServer, mpsc::Receiver<ServerEvent>), Box<dyn Error>> {
    let program = PathBuf::from(std::env::var(SERVER_VARIABLE)?);
    let env = std::env::var_os(JAVA_HOME_VARIABLE)
        .map_or_else(Vec::new, |home| vec![(JAVA_HOME_VARIABLE.to_owned(), home)]);
    let launch = ServerLaunch {
        program,
        args: Vec::new(),
        env,
    };
    let (server, mut events) = LanguageServer::start(
        &HostLink::Local,
        &launch,
        root,
        ServerSettings::new(serde_json::Value::Null),
    )?;
    let (forward, received) = mpsc::channel();
    thread::spawn(move || {
        while let Some(event) = block_on(events.next()) {
            if forward.send(event).is_err() {
                return;
            }
        }
    });
    Ok((server, received))
}

fn await_ready(received: &mpsc::Receiver<ServerEvent>) -> (bool, bool) {
    let (mut progressed, mut refreshed) = (false, false);
    while let Ok(event) = received.recv_timeout(if refreshed { SETTLE } else { READY }) {
        match event {
            ServerEvent::Progress(_) => progressed = true,
            ServerEvent::DiagnosticsRefresh => refreshed = true,
            _ => {}
        }
    }
    (progressed, refreshed)
}

fn reports_the_error(client: &LspClient, document: &TextDocumentIdentifier) -> Outcome {
    let report = block_on(client.request::<DocumentDiagnosticRequest>(
        &DocumentDiagnosticParams {
            text_document: document.clone(),
            identifier: None,
            previous_result_id: None,
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    ))?;
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(full)) = report
    else {
        return Err("expected a full report".into());
    };
    assert!(full
        .full_document_diagnostic_report
        .items
        .iter()
        .any(|item| item.message.contains("missing")));
    Ok(())
}

fn navigates_and_completes(
    client: &LspClient,
    document: &TextDocumentIdentifier,
    text: &str,
) -> Outcome {
    let shout = position(text, "shout", 2).ok_or("no shout call")?;
    let definition = block_on(client.request::<GotoDefinition>(&GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams::new(document.clone(), shout),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }))?;
    assert!(definition.is_some());
    let dot = position(text, "Names.", 6).ok_or("no Names call")?;
    let completion = block_on(client.request::<Completion>(&CompletionParams {
        text_document_position: TextDocumentPositionParams::new(document.clone(), dot),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    }))?;
    let labels: Vec<String> = match completion {
        Some(CompletionResponse::List(list)) => {
            list.items.into_iter().map(|item| item.label).collect()
        }
        Some(CompletionResponse::Array(items)) => {
            items.into_iter().map(|item| item.label).collect()
        }
        None => Vec::new(),
    };
    assert!(labels.iter().any(|label| label == "shout"));
    Ok(())
}

fn renames_the_class_and_its_file(
    client: &LspClient,
    document: &TextDocumentIdentifier,
    text: &str,
) -> Outcome {
    let class = position(text, "Names.", 1).ok_or("no Names reference")?;
    let renamed = block_on(client.request::<Rename>(&RenameParams {
        text_document_position: TextDocumentPositionParams::new(document.clone(), class),
        new_name: "Labels".to_owned(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }))?
    .ok_or("no rename edit")?;
    let Some(DocumentChanges::Operations(operations)) = renamed.document_changes else {
        return Err("expected document operations".into());
    };
    assert!(operations.iter().any(|operation| matches!(
        operation,
        DocumentChangeOperation::Op(ResourceOp::Rename(_))
    )));
    Ok(())
}

fn resolves_a_code_action(
    client: &LspClient,
    document: &TextDocumentIdentifier,
    text: &str,
) -> Outcome {
    let shout = position(text, "shout", 2).ok_or("no shout call")?;
    let actions = block_on(client.request::<CodeActionRequest>(&CodeActionParams {
        text_document: document.clone(),
        range: Range::new(shout, shout),
        context: CodeActionContext::default(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }))?
    .unwrap_or_default();
    if let Some(CodeActionOrCommand::CodeAction(action)) = actions.into_iter().next() {
        let resolved = block_on(client.request::<CodeActionResolveRequest>(&action))?;
        assert_eq!(resolved.title, action.title);
    }
    Ok(())
}

#[test]
#[ignore = "needs a krad-java-server launcher in KRAD_JAVA_SERVER and the Maven sample in KRAD_JAVA_SAMPLE"]
fn krad_java_server_answers_krad_lsp() -> Outcome {
    let root = PathBuf::from(std::env::var(SAMPLE_VARIABLE)?);
    let (server, received) = start(&root)?;
    let client = server.client.clone();
    let capabilities = block_on(LanguageServer::initialize(
        client.clone(),
        &HostLink::Local,
        std::slice::from_ref(&root),
        None,
    ))?;
    assert!(capabilities.diagnostic_provider.is_some() && capabilities.rename_provider.is_some());
    assert_eq!(await_ready(&received), (true, true));
    let main = root.join("app/src/main/java/sample/app/Main.java");
    let text = std::fs::read_to_string(&main)?
        .replace("Names.shout(\"krad\")", "Names.shout(\"krad\") + missing");
    client.document(DocumentChange::Opened {
        path: main.clone(),
        language: "java".to_owned(),
        text: text.clone(),
    })?;
    let document = TextDocumentIdentifier::new(file_uri(&main)?);
    reports_the_error(&client, &document)?;
    navigates_and_completes(&client, &document, &text)?;
    renames_the_class_and_its_file(&client, &document, &text)?;
    resolves_a_code_action(&client, &document, &text)?;
    drop(server);
    Ok(())
}
