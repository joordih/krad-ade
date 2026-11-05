use futures::channel::{mpsc, oneshot};
use futures::executor::block_on;
use futures::StreamExt;
use krad_agents::{
    call, serve, AgentInfo, BusToken, Caller, Envelope, LeadHandle, Request, Response,
    TerminalHandle, TerminalState,
};
use krad_domain::{AgentPreset, AgentState};
use std::io::Write as _;
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

const CHANNEL_CAPACITY: usize = 8;
const LEFT_CALLER_NOTICE: Duration = Duration::from_secs(5);
const CANCEL_POLL: Duration = Duration::from_millis(20);

fn agents() -> Vec<AgentInfo> {
    vec![AgentInfo {
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
    assert!(matches!(response, Response::Error(_)));
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
    drop(stream);
    let started = Instant::now();
    while !incoming.reply.is_canceled() && started.elapsed() < LEFT_CALLER_NOTICE {
        thread::sleep(CANCEL_POLL);
    }
    assert!(
        incoming.reply.is_canceled(),
        "the app learns the hook is gone"
    );
    Ok(())
}
