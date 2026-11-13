#![cfg(unix)]

use futures::executor::block_on;
use futures::StreamExt as _;
use krad_agents::{call, AgentsError, Caller, Request, Response, TerminalHandle, TerminalState};
use krad_domain::{
    AgentPreset, AgentState, EnvironmentData, EnvironmentName, ResumeId, RuntimeState, SessionId,
    SessionLead, TerminalId, TerminalNode, WebhookId, WorkerSpawn,
};
use krad_protocol::{
    AgentRef, BusCommand, Command, Conversation, EventKind, Feed, HookHeard, HookOrigin,
    HookReport, Reply, Scope, TerminalCommand, Typing,
};
use krad_runtime::{
    BusSetup, EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup, TerminalLaunch,
    TerminalPlace,
};
use std::error::Error;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const LEAD: TerminalId = TerminalId(1);
const PLAIN: TerminalId = TerminalId(2);
const WORKER: TerminalId = TerminalId(3);
const SHELL: &str = "/bin/sh";
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);
const TIMED_OUT: &str = "the awaited answer never came";
const NO_BUS: &str = "the runtime serves no bus";
const NO_SESSION: &str = "sessions mode opened no session";
const CALLER_PANICKED: &str = "the caller panicked";
const SCREEN_LINES: usize = 40;
const WAIT_SECS: u64 = 10;

fn node(id: TerminalId) -> Result<TerminalNode, serde_json::Error> {
    serde_json::from_value(serde_json::json!({"id": id.0, "title": "agent", "state": "Idle"}))
}

fn agent(terminal: TerminalId) -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        terminal,
    }
}

fn led_environment() -> Result<(EnvironmentData, SessionId), Box<dyn Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = data.add_group(std::env::temp_dir(), None);
    data.nodes = [LEAD, PLAIN, WORKER]
        .into_iter()
        .map(node)
        .collect::<Result<_, _>>()?;
    for node in &mut data.nodes {
        node.group = Some(group);
    }
    data.enter_sessions();
    let session = data.active_session.ok_or(NO_SESSION)?;
    if let Some(led) = data.sessions.iter_mut().find(|led| led.id == session) {
        led.lead = Some(SessionLead {
            terminal: Some(LEAD),
            ..SessionLead::new(AgentPreset::ClaudeCode, None)
        });
    }
    let spawn = WorkerSpawn {
        lead: Some(session),
        task: "Print the answer".to_owned(),
    };
    data.open_worker_session(WORKER, spawn);
    Ok((data, session))
}

fn shell() -> TerminalLaunch {
    TerminalLaunch {
        program: SHELL.to_owned(),
        args: Vec::new(),
        cwd: None,
        vars: vec![("PS1".to_owned(), "$ ".to_owned())],
        place: TerminalPlace::Here,
        harness: None,
        prefill: Vec::new(),
        typed: None,
        tap: None,
    }
}

fn start() -> Result<(RuntimeHandle, Receiver<Feed>), Box<dyn Error>> {
    let (data, _) = led_environment()?;
    let runtime = Runtime::start(RuntimeSetup {
        bus: Some(BusSetup::default()),
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut feed = runtime.subscribe();
    let (heard, received) = channel();
    thread::spawn(move || {
        while let Some(next) = block_on(feed.next()) {
            if heard.send(next).is_err() {
                break;
            }
        }
    });
    Ok((runtime, received))
}

fn call_from(
    runtime: &RuntimeHandle,
    terminal: TerminalId,
    request: Request,
) -> Result<JoinHandle<Result<Response, AgentsError>>, Box<dyn Error>> {
    let endpoint = &runtime.bus().ok_or(NO_BUS)?.endpoint;
    let address = endpoint.address.to_string();
    let token = endpoint.token.clone();
    let caller = Caller::Terminal(TerminalHandle {
        environment: ENVIRONMENT.to_owned(),
        terminal: terminal.0,
    });
    Ok(thread::spawn(move || {
        call(&address, &token, &caller, request)
    }))
}

fn asked(
    runtime: &RuntimeHandle,
    terminal: TerminalId,
    request: Request,
) -> Result<Response, Box<dyn Error>> {
    let asked = call_from(runtime, terminal, request)?;
    Ok(asked.join().map_err(|_| CALLER_PANICKED)??)
}

fn heard_hook(feed: &Receiver<Feed>) -> Result<HookHeard, Box<dyn Error>> {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match feed.recv_timeout(POLL) {
            Ok(Feed::Event(event)) => {
                if let EventKind::AgentHooked(heard) = event.kind {
                    return Ok(heard);
                }
            }
            Ok(Feed::Snapshot(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("the feed ended".into()),
        }
    }
    Err(TIMED_OUT.into())
}

fn hook(event: &str, tool: Option<&str>, resume: &str) -> Request {
    Request::Hook {
        harness: AgentPreset::ClaudeCode,
        event: event.to_owned(),
        tool: tool.map(str::to_owned),
        detail: Some("Reading the tests".to_owned()),
        resume: Some(resume.to_owned()),
        mode: None,
    }
}

fn node_in(
    runtime: &RuntimeHandle,
    id: TerminalId,
) -> Result<Option<TerminalNode>, Box<dyn Error>> {
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?;
    Ok(snapshot
        .environments
        .into_iter()
        .flat_map(|data| data.nodes)
        .find(|node| node.id == id))
}

fn type_into(runtime: &RuntimeHandle, terminal: TerminalId, typing: Typing) -> TestResult {
    let typed = Command::Terminal(TerminalCommand::Type {
        terminal: agent(terminal),
        typing,
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), typed))?,
        Reply::Accepted
    );
    Ok(())
}

fn screen_shows(
    runtime: &RuntimeHandle,
    reader: TerminalId,
    target: TerminalId,
    text: &str,
) -> TestResult {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let read = Request::Read {
            target: target.0,
            lines: SCREEN_LINES,
        };
        if let Response::Screen(screen) = asked(runtime, reader, read)? {
            if screen.contains(text) {
                return Ok(());
            }
        }
        thread::sleep(POLL);
    }
    Err(format!("the screen never showed {text}").into())
}

fn settled(runtime: &RuntimeHandle, target: TerminalId) -> Result<Response, Box<dyn Error>> {
    let wait = Request::Wait {
        target: target.0,
        timeout_secs: WAIT_SECS,
    };
    asked(runtime, PLAIN, wait)
}

#[test]
fn a_terminal_hook_is_answered_by_the_runtime_which_keeps_its_resume_and_tells_clients(
) -> TestResult {
    let (runtime, feed) = start()?;
    drop(runtime.open_terminal(agent(PLAIN), shell()));

    let started = asked(&runtime, PLAIN, hook("UserPromptSubmit", None, "first-a1"))?;
    assert_eq!(started, Response::Noted);
    let heard = heard_hook(&feed)?;
    assert_eq!(heard.agent, agent(PLAIN));
    assert_eq!(heard.state, Some(AgentState::Working));
    assert_eq!(heard.activity.as_deref(), Some("Reading the tests"));
    assert_eq!(heard.conversation, Conversation::Kept);
    assert_eq!(heard.origin, HookOrigin::Live);
    let kept = node_in(&runtime, PLAIN)?.ok_or("the node is gone")?;
    assert_eq!(kept.resume, Some("first-a1".parse::<ResumeId>()?));
    assert_eq!(kept.harness, Some(AgentPreset::ClaudeCode));

    let fresh = asked(
        &runtime,
        PLAIN,
        hook("SessionStart", Some("startup"), "second-b2"),
    )?;
    assert_eq!(fresh, Response::Noted);
    assert_eq!(heard_hook(&feed)?.conversation, Conversation::StartedFresh);

    let replay = Command::Bus(BusCommand::ReplayHook {
        agent: agent(PLAIN),
        report: HookReport {
            harness: AgentPreset::ClaudeCode,
            event: "Stop".to_owned(),
            tool: None,
            detail: None,
            resume: None,
            mode: None,
        },
        delivered: [WebhookId(7)].into_iter().collect(),
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), replay))?,
        Reply::Accepted
    );
    let replayed = heard_hook(&feed)?;
    assert_eq!(replayed.state, Some(AgentState::Done));
    assert_eq!(
        replayed.origin,
        HookOrigin::Replayed {
            delivered: [WebhookId(7)].into_iter().collect()
        }
    );
    Ok(())
}

#[test]
fn agents_type_into_wait_on_and_read_runtime_terminals_and_a_lead_sends_to_its_worker() -> TestResult
{
    let (runtime, _feed) = start()?;
    drop(runtime.open_terminal(agent(PLAIN), shell()));
    drop(runtime.open_terminal(agent(WORKER), shell()));

    type_into(
        &runtime,
        WORKER,
        Typing::Submit {
            text: "echo worker-$((40 + 2))".to_owned(),
        },
    )?;
    screen_shows(&runtime, PLAIN, WORKER, "worker-42")?;
    assert_eq!(
        settled(&runtime, WORKER)?,
        Response::Settled {
            target: WORKER.0,
            state: TerminalState::Running(AgentState::Idle),
        }
    );

    let sent = asked(
        &runtime,
        LEAD,
        Request::Send {
            to: WORKER.0,
            body: "echo sent-$((5 + 5))".to_owned(),
        },
    )?;
    assert_eq!(sent, Response::Submitted { terminal: WORKER.0 });
    screen_shows(&runtime, PLAIN, WORKER, "sent-10")?;

    type_into(
        &runtime,
        PLAIN,
        Typing::Steps {
            steps: vec![b"echo steps-$((1 + 2))".to_vec(), b"\r".to_vec()],
        },
    )?;
    screen_shows(&runtime, WORKER, PLAIN, "steps-3")?;

    type_into(
        &runtime,
        WORKER,
        Typing::Notice {
            text: "exit".to_owned(),
        },
    )?;
    let deadline = Instant::now() + WAIT;
    loop {
        let answer = settled(&runtime, WORKER)?;
        if answer
            == (Response::Settled {
                target: WORKER.0,
                state: TerminalState::Exited,
            })
        {
            break;
        }
        if Instant::now() > deadline {
            return Err(format!("the worker never exited: {answer:?}").into());
        }
        thread::sleep(POLL);
    }
    Ok(())
}

#[test]
fn a_lead_reading_its_finished_terminal_worker_closes_it() -> TestResult {
    let (runtime, _feed) = start()?;
    drop(runtime.open_terminal(agent(WORKER), shell()));
    type_into(
        &runtime,
        WORKER,
        Typing::Submit {
            text: "echo done-$((6 * 7))".to_owned(),
        },
    )?;
    screen_shows(&runtime, PLAIN, WORKER, "done-42")?;
    let deadline = Instant::now() + WAIT;
    loop {
        let read = Request::Read {
            target: WORKER.0,
            lines: SCREEN_LINES,
        };
        let tail = asked(&runtime, LEAD, read)?;
        assert!(matches!(tail, Response::Screen(_)), "{tail:?}");
        if node_in(&runtime, WORKER)?.is_none() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("the finished worker stayed open".into());
        }
        thread::sleep(POLL);
    }
}
