use futures::executor::block_on;
use futures::StreamExt as _;
use lsp_types::request::Shutdown;
use serde_json::{json, Value};
use std::io::{pipe, BufReader, PipeReader, PipeWriter};
use std::path::PathBuf;
use std::thread;

use super::*;
use crate::rpc::{write_message, Incoming, Message};

struct FakeServer {
    from_client: BufReader<PipeReader>,
    to_client: PipeWriter,
}

impl FakeServer {
    fn next(&mut self) -> Option<Incoming> {
        read_message(&mut self.from_client).ok().flatten()
    }

    fn answer(&mut self, id: &Value, result: &Value) {
        assert!(write_message(&mut self.to_client, Message::Reply { id, result }).is_ok());
    }
}

fn connect() -> Result<(LspClient, mpsc::Receiver<ServerEvent>, FakeServer), LspError> {
    let (client_reads, server_writes) = pipe()?;
    let (server_reads, client_writes) = pipe()?;
    let (client, events) = LspClient::over(
        BufReader::new(client_reads),
        client_writes,
        ServerSettings::new(json!({"rust-analyzer": {"checkOnSave": true}})),
    )?;
    let server = FakeServer {
        from_client: BufReader::new(server_reads),
        to_client: server_writes,
    };
    Ok((client, events, server))
}

#[test]
fn a_request_resolves_with_the_server_answer() -> Result<(), LspError> {
    let (client, _events, mut server) = connect()?;
    let answering = thread::spawn(move || {
        if let Some(Incoming::Request { id, method, .. }) = server.next() {
            assert_eq!(method, "shutdown");
            server.answer(&id, &Value::Null);
        }
        server
    });
    block_on(client.request::<Shutdown>(&()))?;
    assert!(answering.join().is_ok());
    Ok(())
}

#[test]
fn edits_raise_the_document_version() -> Result<(), LspError> {
    let (client, _events, mut server) = connect()?;
    let path = PathBuf::from("/work/main.rs");
    client.document(DocumentChange::Opened {
        path: path.clone(),
        language: "rust".to_owned(),
        text: "fn main() {}".to_owned(),
    })?;
    client.document(DocumentChange::Edited {
        path,
        text: "fn main() { }".to_owned(),
    })?;
    let opened = server.next();
    let edited = server.next();
    assert!(
        matches!(opened, Some(Incoming::Notification { method, params })
        if method == "textDocument/didOpen" && params["textDocument"]["version"] == json!(0))
    );
    assert!(
        matches!(edited, Some(Incoming::Notification { method, params })
        if method == "textDocument/didChange" && params["textDocument"]["version"] == json!(1))
    );
    Ok(())
}

#[test]
fn the_workspace_folders_sent_at_initialize_answer_a_later_question() -> Result<(), LspError> {
    use lsp_types::request::Initialize;
    use lsp_types::{InitializeParams, WorkspaceFolder};

    let (client, _events, mut server) = connect()?;
    let folder = WorkspaceFolder {
        uri: crate::uri::file_uri(std::path::Path::new("/work/loader"))?,
        name: "loader".to_owned(),
    };
    let params = InitializeParams {
        workspace_folders: Some(vec![folder]),
        ..InitializeParams::default()
    };
    let initializing = thread::spawn(move || block_on(client.request::<Initialize>(&params)));
    let Some(Incoming::Request { id, .. }) = server.next() else {
        return Err(LspError::Stopped);
    };
    assert!(write_message(
        &mut server.to_client,
        Message::Request {
            id: &json!(9),
            method: "workspace/workspaceFolders",
            params: &Value::Null,
        },
    )
    .is_ok());
    assert!(
        matches!(server.next(), Some(Incoming::Response { id: 9, outcome: Ok(Value::Array(folders)) })
        if folders[0]["name"] == json!("loader"))
    );
    server.answer(&id, &json!({"capabilities": {}}));
    assert!(initializing.join().is_ok_and(|answer| answer.is_ok()));
    Ok(())
}

#[test]
fn diagnostics_arrive_as_events_and_configuration_is_answered() -> Result<(), LspError> {
    let (_client, mut events, mut server) = connect()?;
    let ask = json!({"items": [{"section": "rust-analyzer.checkOnSave"}, {"section": "other"}]});
    let notice = json!({"uri": "file:///work/main.rs", "diagnostics": []});
    assert!(write_message(
        &mut server.to_client,
        Message::Request {
            id: &json!(3),
            method: "workspace/configuration",
            params: &ask,
        },
    )
    .is_ok());
    assert!(write_message(
        &mut server.to_client,
        Message::Notification {
            method: "textDocument/publishDiagnostics",
            params: &notice,
        },
    )
    .is_ok());
    assert!(
        matches!(server.next(), Some(Incoming::Response { id: 3, outcome: Ok(Value::Array(items)) })
        if items == [json!(true), Value::Null])
    );
    assert!(matches!(
        block_on(events.next()),
        Some(ServerEvent::Diagnostics(_))
    ));
    Ok(())
}

#[test]
fn new_settings_are_announced_and_answer_later_questions() -> Result<(), LspError> {
    let (client, _events, mut server) = connect()?;
    client.reconfigure(ServerSettings::new(
        json!({"python": {"pythonPath": "/srv/app/.venv/bin/python"}}),
    ))?;
    assert!(
        matches!(server.next(), Some(Incoming::Notification { method, params })
        if method == "workspace/didChangeConfiguration"
            && params["settings"]["python"]["pythonPath"] == json!("/srv/app/.venv/bin/python"))
    );
    let ask = json!({"items": [{"section": "python.pythonPath"}]});
    assert!(write_message(
        &mut server.to_client,
        Message::Request {
            id: &json!(4),
            method: "workspace/configuration",
            params: &ask,
        },
    )
    .is_ok());
    assert!(
        matches!(server.next(), Some(Incoming::Response { id: 4, outcome: Ok(Value::Array(items)) })
        if items == [json!("/srv/app/.venv/bin/python")])
    );
    Ok(())
}

#[test]
fn pending_requests_fail_once_the_server_stops() -> Result<(), LspError> {
    let (client, mut events, mut server) = connect()?;
    let waiting = client.request::<Shutdown>(&());
    assert!(matches!(server.next(), Some(Incoming::Request { .. })));
    drop(server);
    assert!(matches!(block_on(waiting), Err(LspError::Stopped)));
    assert_eq!(block_on(events.next()), Some(ServerEvent::Stopped));
    Ok(())
}
