use futures::channel::{mpsc, oneshot};
use futures::executor::block_on;
use futures::StreamExt;
use krad_agents::{
    call, serve, AgentInfo, ApiError, ApiVersion, BusToken, Caller, ChangeSet, ChangedFile,
    Envelope, ErrorCode, FilePosition, LeadHandle, Request, Response, SessionInfo, TerminalHandle,
    TerminalState, WorktreeInfo, API_VERSION,
};
use krad_domain::ChangeKind;
use krad_domain::{AgentPreset, AgentState};
use std::io::Write as _;
use std::io::{BufRead as _, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

const CHANNEL_CAPACITY: usize = 8;
const LEFT_CALLER_NOTICE: Duration = Duration::from_secs(5);

fn agents() -> Vec<AgentInfo> {
    vec![AgentInfo {
        environment: None,
        terminal: 4,
        title: "Codex · 4".to_owned(),
        group: Some("api".to_owned()),
        state: TerminalState::Running(AgentState::Working),
        harness: Some(AgentPreset::Codex),
        is_self: false,
    }]
}

fn caller() -> Caller {
    Caller::Terminal(TerminalHandle {
        environment: "main".to_owned(),
        terminal: 1,
    })
}

fn fake_app(mut requests: mpsc::Receiver<krad_agents::Incoming>) {
    thread::spawn(move || {
        let mut unanswered: Vec<oneshot::Sender<Response>> = Vec::new();
        while let Some(incoming) = block_on(requests.next()) {
            match incoming.request {
                Request::List => {
                    assert!(incoming.reply.send(Response::Agents(agents())).is_ok());
                }
                Request::Spawn { .. } => {
                    let spawned = Response::Spawned { terminal: 5 };
                    assert!(incoming.reply.send(spawned).is_ok());
                }
                request @ (Request::Sessions
                | Request::CreateSession { .. }
                | Request::CloseSession { .. }
                | Request::CreateWorktree { .. }
                | Request::OpenFile { .. }
                | Request::Changes { .. }) => {
                    assert!(incoming.reply.send(workspace_answer(request)).is_ok());
                }
                _ => unanswered.push(incoming.reply),
            }
        }
    });
}

#[test]
fn serves_requests_and_times_out_unanswered_ones() -> Result<(), krad_agents::AgentsError> {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    fake_app(receiver);
    let address = endpoint.address.to_string();

    let listed = call(&address, &endpoint.token, &caller(), Request::List)?;
    assert_eq!(listed, Response::Agents(agents()));

    let check = Request::Check { wait_secs: Some(1) };
    assert_eq!(
        call(&address, &endpoint.token, &caller(), check)?,
        Response::TimedOut
    );
    Ok(())
}

#[test]
fn rejects_wrong_token() -> Result<(), krad_agents::AgentsError> {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    fake_app(receiver);

    let response = call(
        &endpoint.address.to_string(),
        &BusToken::from("not-the-token"),
        &caller(),
        Request::List,
    )?;
    assert!(matches!(
        response,
        Response::Error(ApiError {
            code: ErrorCode::Unauthorized,
            ..
        })
    ));
    Ok(())
}

fn lead() -> Caller {
    Caller::Lead(LeadHandle {
        environment: "main".to_owned(),
        session: 2,
    })
}

fn spawn() -> Request {
    Request::Spawn {
        agent: AgentPreset::Codex,
        worktree: None,
        task: "Replay-protection tests".to_owned(),
    }
}

#[test]
fn a_worker_cannot_call_as_the_lead() -> Result<(), krad_agents::AgentsError> {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    fake_app(receiver);
    let address = endpoint.address.to_string();

    let posing = call(&address, &endpoint.token, &lead(), spawn())?;
    assert!(matches!(posing, Response::Error(_)), "{posing:?}");
    let listed = call(&address, &endpoint.lead_token, &caller(), Request::List)?;
    assert!(matches!(listed, Response::Error(_)), "{listed:?}");
    Ok(())
}

#[test]
fn a_caller_that_leaves_cancels_its_waiting_request() -> Result<(), krad_agents::AgentsError> {
    let (sender, mut receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    let envelope = Envelope {
        api_version: API_VERSION,
        token: endpoint.token.clone(),
        from: caller(),
        request: Request::Ask {
            tool: "Bash".to_owned(),
            input: serde_json::Value::Null,
            suggestions: Vec::new(),
            call: None,
        },
    };
    let mut line = serde_json::to_string(&envelope)?;
    line.push('\n');
    let mut stream = TcpStream::connect(endpoint.address)?;
    stream.write_all(line.as_bytes())?;
    let Some(incoming) = block_on(receiver.next()) else {
        unreachable!("the server hands the ask to the app");
    };
    assert!(!incoming.reply.is_canceled());
    let (noticed, gone) = std_mpsc::channel();
    let mut reply = incoming.reply;
    thread::spawn(move || {
        block_on(reply.cancellation());
        noticed.send(()).unwrap_or_default();
    });
    drop(stream);
    assert!(
        gone.recv_timeout(LEFT_CALLER_NOTICE).is_ok(),
        "the app learns the hook is gone"
    );
    Ok(())
}

fn workspace_answer(request: Request) -> Response {
    match request {
        Request::Sessions => Response::Sessions(vec![session()]),
        Request::CreateSession { .. } => Response::SessionCreated { session: 3 },
        Request::CloseSession { session: 9 } => {
            Response::error(ErrorCode::NotFound, "No session with that id")
        }
        Request::CloseSession { session } => Response::SessionClosed { session },
        Request::CreateWorktree { name } => Response::WorktreeCreated(Box::new(WorktreeInfo {
            path: PathBuf::from("/repos").join(&name),
            group: name.clone(),
            branch: Some(name),
        })),
        Request::OpenFile { path, .. } => Response::Opened { path },
        Request::Changes { session } => Response::Changes(Box::new(changes(
            session.map_or_else(|| "api".to_owned(), |session| format!("session {session}")),
        ))),
        _ => Response::error(ErrorCode::Unsupported, "not a workspace request"),
    }
}

fn session() -> SessionInfo {
    SessionInfo {
        session: 2,
        name: "Main".to_owned(),
        group: Some("api".to_owned()),
        is_current: true,
        terminals: vec![4],
    }
}

fn changes(group: String) -> ChangeSet {
    ChangeSet {
        group,
        root: PathBuf::from("/repos/api"),
        branch: Some("main".to_owned()),
        files: vec![ChangedFile {
            path: PathBuf::from("src/lib.rs"),
            kind: ChangeKind::Modified,
        }],
        patch: "--- a/src/lib.rs\n+++ b/src/lib.rs\n".to_owned(),
    }
}

fn raw_call(
    endpoint: &krad_agents::Endpoint,
    line: &str,
) -> Result<Response, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(endpoint.address)?;
    stream.write_all(format!("{line}\n").as_bytes())?;
    let mut answer = String::new();
    BufReader::new(&stream).read_line(&mut answer)?;
    Ok(serde_json::from_str(&answer)?)
}

#[test]
fn the_handshake_answers_the_api_version_without_the_app() -> Result<(), Box<dyn std::error::Error>>
{
    let (sender, _receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    let address = endpoint.address.to_string();
    let Response::Hello(info) = call(&address, &endpoint.token, &caller(), Request::Hello)? else {
        unreachable!("the bus answers the handshake");
    };
    assert_eq!(info.api_version, API_VERSION);
    assert_eq!(info.build, krad_host::HOST_CLI_BUILD);
    Ok(())
}

#[test]
fn a_caller_of_another_api_version_is_refused_with_its_code(
) -> Result<(), Box<dyn std::error::Error>> {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    fake_app(receiver);
    let mut envelope = Envelope {
        api_version: ApiVersion(API_VERSION.0 + 1),
        token: endpoint.token.clone(),
        from: caller(),
        request: Request::List,
    };
    let refused = raw_call(&endpoint, &serde_json::to_string(&envelope)?)?;
    assert_eq!(
        refused,
        Response::Error(ApiError::unsupported_version(envelope.api_version))
    );
    envelope.api_version = API_VERSION;
    let unversioned = serde_json::to_string(&envelope)?.replace(r#""api_version":1,"#, "");
    assert!(!unversioned.contains("api_version"));
    assert_eq!(
        raw_call(&endpoint, &unversioned)?,
        Response::Agents(agents())
    );
    let malformed = raw_call(&endpoint, "{not json")?;
    assert!(matches!(
        malformed,
        Response::Error(ApiError {
            code: ErrorCode::BadRequest,
            ..
        })
    ));
    Ok(())
}

#[test]
fn workspace_requests_reach_the_app_and_come_back() -> Result<(), krad_agents::AgentsError> {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    fake_app(receiver);
    let address = endpoint.address.to_string();
    let ask = |request| call(&address, &endpoint.token, &caller(), request);

    assert_eq!(ask(Request::Sessions)?, Response::Sessions(vec![session()]));
    assert_eq!(
        ask(Request::CreateSession {
            name: None,
            group: None
        })?,
        Response::SessionCreated { session: 3 }
    );
    assert_eq!(
        ask(Request::CloseSession { session: 3 })?,
        Response::SessionClosed { session: 3 }
    );
    assert!(matches!(
        ask(Request::CloseSession { session: 9 })?,
        Response::Error(ApiError {
            code: ErrorCode::NotFound,
            ..
        })
    ));
    let Response::WorktreeCreated(worktree) = ask(Request::CreateWorktree {
        name: "fix-login".to_owned(),
    })?
    else {
        unreachable!("the app creates the worktree");
    };
    assert_eq!(worktree.branch.as_deref(), Some("fix-login"));
    let open = Request::OpenFile {
        path: PathBuf::from("src/lib.rs"),
        position: Some(FilePosition {
            line: 3,
            column: None,
        }),
    };
    assert_eq!(
        ask(open)?,
        Response::Opened {
            path: PathBuf::from("src/lib.rs")
        }
    );
    assert_eq!(
        ask(Request::Changes { session: Some(7) })?,
        Response::Changes(Box::new(changes("session 7".to_owned())))
    );
    Ok(())
}
