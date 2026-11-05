use super::*;
use crate::acp::rpc::ErrorCode;
use crate::acp::{
    ClientCapabilities, ContentBlock, FileSystemCapabilities, Implementation, McpServer,
    PermissionOptionKind, ProtocolVersion, ReadTextFileResponse, RequestPermissionOutcome,
    RequestPermissionResponse, SessionId, SessionUpdate, StopReason, TextContent, ToolCallContent,
    ToolCallStatus, ToolKind,
};
use serde_json::{json, Value};
use std::error::Error;
use std::io::{pipe, BufRead, BufReader, PipeReader, PipeWriter};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;

const WAIT: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(200);

#[derive(Debug)]
enum Seen {
    Update(SessionNotification),
    Request(AgentRequest),
    Closed,
}

struct FakeClient {
    seen: Sender<Seen>,
}

impl AcpClient for FakeClient {
    fn session_update(&mut self, notification: SessionNotification) {
        self.seen
            .send(Seen::Update(notification))
            .unwrap_or_default();
    }

    fn request(&mut self, request: AgentRequest) {
        match request {
            AgentRequest::ReadTextFile(read, responder) => {
                responder.reply(&ReadTextFileResponse {
                    content: format!("contents of {}", read.path.display()),
                });
            }
            AgentRequest::RequestPermission(ask, responder) => {
                let Some(allow) = ask
                    .options
                    .into_iter()
                    .find(|option| option.kind == PermissionOptionKind::AllowOnce)
                else {
                    return;
                };
                responder.reply(&RequestPermissionResponse {
                    outcome: RequestPermissionOutcome::Selected {
                        option_id: allow.option_id,
                    },
                });
            }
            other => self.seen.send(Seen::Request(other)).unwrap_or_default(),
        }
    }

    fn closed(&mut self) {
        self.seen.send(Seen::Closed).unwrap_or_default();
    }
}

struct FakeAgent {
    reads: BufReader<PipeReader>,
    writes: PipeWriter,
}

impl FakeAgent {
    fn receive(&mut self) -> Result<Value, Box<dyn Error>> {
        let mut line = String::new();
        self.reads.read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    }

    fn send(&mut self, line: &str) -> TestResult {
        writeln!(self.writes, "{line}")?;
        Ok(())
    }
}

fn connect() -> Result<(AcpConnection, FakeAgent, Receiver<Seen>), Box<dyn Error>> {
    let (client_reads, agent_writes) = pipe()?;
    let (agent_reads, client_writes) = pipe()?;
    let (seen, events) = channel();
    let connection = AcpConnection::start(client_reads, client_writes, FakeClient { seen });
    let agent = FakeAgent {
        reads: BufReader::new(agent_reads),
        writes: agent_writes,
    };
    Ok((connection, agent, events))
}

fn next_update(events: &Receiver<Seen>) -> Result<SessionUpdate, Box<dyn Error>> {
    match events.recv_timeout(WAIT)? {
        Seen::Update(notification) => Ok(notification.update),
        other => unreachable!("expected an update, got {other:?}"),
    }
}

fn session() -> SessionId {
    SessionId("sess_abc123def456".to_owned())
}

#[test]
fn the_initialize_handshake_offers_the_clients_capabilities_and_reads_the_agents() -> TestResult {
    let (connection, mut agent, _events) = connect()?;
    let pending = connection.initialize(&InitializeRequest {
        protocol_version: ProtocolVersion::V1,
        client_capabilities: ClientCapabilities {
            fs: FileSystemCapabilities {
                read_text_file: true,
                write_text_file: true,
            },
            terminal: true,
            ..ClientCapabilities::default()
        },
        client_info: Some(Implementation {
            name: "krad_ade".to_owned(),
            title: Some("Krad ADE".to_owned()),
            version: "1.0.0".to_owned(),
        }),
    });
    assert_eq!(
        agent.receive()?,
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": {"readTextFile": true, "writeTextFile": true},
                    "terminal": true,
                    "auth": {"terminal": false}
                },
                "clientInfo": {"name": "krad_ade", "title": "Krad ADE", "version": "1.0.0"}
            }
        })
    );
    agent.send(
        r#"{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"promptCapabilities":{"image":true,"audio":true,"embeddedContext":true},"mcpCapabilities":{"http":true,"sse":true},"sessionCapabilities":{}},"agentInfo":{"name":"my-agent","title":"My Agent","version":"1.0.0"},"authMethods":[]}}"#,
    )?;
    let response = pending.wait_timeout(WAIT)?;
    assert_eq!(response.protocol_version, ProtocolVersion::V1);
    assert!(response.agent_capabilities.load_session);
    assert!(
        response
            .agent_capabilities
            .prompt_capabilities
            .embedded_context
    );
    assert!(response.agent_capabilities.mcp_capabilities.sse);
    assert_eq!(
        response.agent_info.map(|info| info.name),
        Some("my-agent".to_owned())
    );
    Ok(())
}

#[test]
fn a_prompt_streams_thoughts_text_tool_calls_and_a_plan_before_its_stop_reason() -> TestResult {
    let (connection, mut agent, events) = connect()?;
    let pending = connection.prompt(&PromptRequest {
        session_id: session(),
        prompt: vec![ContentBlock::Text(TextContent {
            text: "Can you analyze this code for potential issues?".to_owned(),
        })],
    });
    let sent = agent.receive()?;
    assert_eq!(sent["method"], "session/prompt");
    assert_eq!(sent["params"]["sessionId"], "sess_abc123def456");
    assert_eq!(sent["params"]["prompt"][0]["type"], "text");
    for line in [
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess_abc123def456","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"Looking at the code first."}}}}"#,
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess_abc123def456","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"I'll analyze your code for potential issues."}}}}"#,
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess_abc123def456","update":{"sessionUpdate":"tool_call","toolCallId":"call_001","title":"Analyzing Python code","kind":"other","status":"pending"}}}"#,
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess_abc123def456","update":{"sessionUpdate":"tool_call_update","toolCallId":"call_001","status":"completed","content":[{"type":"content","content":{"type":"text","text":"Analysis complete: no issues found."}}]}}}"#,
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess_abc123def456","update":{"sessionUpdate":"plan","entries":[{"content":"Check for syntax errors","priority":"high","status":"pending"},{"content":"Identify potential type issues","priority":"medium","status":"pending"}]}}}"#,
    ] {
        agent.send(line)?;
    }
    agent.send(r#"{"jsonrpc":"2.0","id":0,"result":{"stopReason":"end_turn"}}"#)?;
    assert!(matches!(
        next_update(&events)?,
        SessionUpdate::AgentThoughtChunk(chunk)
            if chunk.content == ContentBlock::Text(TextContent { text: "Looking at the code first.".to_owned() })
    ));
    assert!(matches!(
        next_update(&events)?,
        SessionUpdate::AgentMessageChunk(_)
    ));
    assert!(matches!(
        next_update(&events)?,
        SessionUpdate::ToolCall(call)
            if call.kind == ToolKind::Other && call.status == ToolCallStatus::Pending
    ));
    assert!(matches!(
        next_update(&events)?,
        SessionUpdate::ToolCallUpdate(update)
            if update.status == Some(ToolCallStatus::Completed)
                && matches!(update.content.as_deref(), Some([ToolCallContent::Content { .. }]))
    ));
    assert!(matches!(
        next_update(&events)?,
        SessionUpdate::Plan(plan) if plan.entries.len() == 2
    ));
    assert_eq!(pending.wait_timeout(WAIT)?.stop_reason, StopReason::EndTurn);
    Ok(())
}

#[test]
fn a_permission_request_is_answered_with_the_option_the_client_picked() -> TestResult {
    let (_connection, mut agent, _events) = connect()?;
    agent.send(
        r#"{"jsonrpc":"2.0","id":5,"method":"session/request_permission","params":{"sessionId":"sess_abc123def456","toolCall":{"toolCallId":"call_001"},"options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]}}"#,
    )?;
    assert_eq!(
        agent.receive()?,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "result": {"outcome": {"outcome": "selected", "optionId": "allow-once"}}
        })
    );
    Ok(())
}

#[test]
fn a_file_read_from_the_agent_is_answered_by_the_client() -> TestResult {
    let (_connection, mut agent, _events) = connect()?;
    agent.send(
        r#"{"jsonrpc":"2.0","id":"read-1","method":"fs/read_text_file","params":{"sessionId":"sess_abc123def456","path":"/home/user/project/src/main.py","line":10,"limit":50}}"#,
    )?;
    assert_eq!(
        agent.receive()?,
        json!({
            "jsonrpc": "2.0",
            "id": "read-1",
            "result": {"content": "contents of /home/user/project/src/main.py"}
        })
    );
    Ok(())
}

#[test]
fn an_error_answer_reaches_the_caller_with_its_code() -> TestResult {
    let (connection, mut agent, _events) = connect()?;
    let pending = connection.new_session(&NewSessionRequest {
        cwd: PathBuf::from("/home/user/project"),
        mcp_servers: Vec::<McpServer>::new(),
    });
    let sent = agent.receive()?;
    assert_eq!(
        sent["params"],
        json!({"cwd": "/home/user/project", "mcpServers": []})
    );
    agent.send(
        r#"{"jsonrpc":"2.0","id":0,"error":{"code":-32000,"message":"Authentication required"}}"#,
    )?;
    let error = pending.wait_timeout(WAIT);
    assert!(matches!(
        error,
        Err(AcpError::Refused(RpcError { code: ErrorCode::AUTH_REQUIRED, ref message, .. }))
            if message == "Authentication required"
    ));
    Ok(())
}

#[test]
fn unknown_updates_and_stray_lines_are_skipped_without_breaking_the_stream() -> TestResult {
    let (_connection, mut agent, events) = connect()?;
    for line in [
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess_abc123def456","update":{"sessionUpdate":"hologram_update","depth":3}}}"#,
        "this is not json",
        r#"{"jsonrpc":"2.0","method":"_vendor/hint","params":{}}"#,
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess_abc123def456","update":{"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":"Still here","annotations":{"priority":1}}},"_meta":{"x":1}}}"#,
    ] {
        agent.send(line)?;
    }
    assert!(matches!(
        next_update(&events)?,
        SessionUpdate::AgentMessageChunk(chunk)
            if chunk.content == ContentBlock::Text(TextContent { text: "Still here".to_owned() })
    ));
    assert!(events.recv_timeout(QUIET).is_err());
    Ok(())
}

#[test]
fn requests_the_client_cannot_or_does_not_answer_still_get_an_error() -> TestResult {
    let (_connection, mut agent, events) = connect()?;
    agent.send(r#"{"jsonrpc":"2.0","id":1,"method":"elicitation/create","params":{}}"#)?;
    assert_eq!(agent.receive()?["error"]["code"], -32601);
    agent.send(r#"{"jsonrpc":"2.0","id":2,"method":"fs/read_text_file","params":{"path":3}}"#)?;
    assert_eq!(agent.receive()?["error"]["code"], -32602);
    agent.send(
        r#"{"jsonrpc":"2.0","id":3,"method":"terminal/create","params":{"sessionId":"sess_abc123def456","command":"npm","args":["test"]}}"#,
    )?;
    let Seen::Request(AgentRequest::CreateTerminal(create, responder)) =
        events.recv_timeout(WAIT)?
    else {
        unreachable!("expected a terminal to create");
    };
    assert_eq!(create.args, ["test"]);
    drop(responder);
    let answer = agent.receive()?;
    assert_eq!(answer["id"], 3);
    assert_eq!(answer["error"]["code"], -32603);
    Ok(())
}

#[test]
fn a_cancel_is_a_notification_without_an_id() -> TestResult {
    let (connection, mut agent, _events) = connect()?;
    connection.cancel(&CancelNotification {
        session_id: session(),
    });
    assert_eq!(
        agent.receive()?,
        json!({
            "jsonrpc": "2.0",
            "method": "session/cancel",
            "params": {"sessionId": "sess_abc123def456"}
        })
    );
    Ok(())
}

#[test]
fn when_the_agent_exits_waiting_calls_end_and_the_client_hears_it() -> TestResult {
    let (connection, mut agent, events) = connect()?;
    let pending = connection.prompt(&PromptRequest {
        session_id: session(),
        prompt: Vec::new(),
    });
    agent.receive()?;
    drop(agent);
    assert!(matches!(pending.wait_timeout(WAIT), Err(AcpError::Gone)));
    assert!(matches!(events.recv_timeout(WAIT), Ok(Seen::Closed)));
    let late = connection.set_mode(&SetSessionModeRequest {
        session_id: session(),
        mode_id: crate::acp::ModeId("code".to_owned()),
    });
    assert!(matches!(late.wait_timeout(WAIT), Err(AcpError::Gone)));
    Ok(())
}
