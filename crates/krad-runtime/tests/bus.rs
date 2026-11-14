use futures::executor::block_on;
use futures::StreamExt as _;
use krad_agents::{
    call, AgentInfo, AgentStatus, AgentsError, Caller, Request, Response, TerminalHandle,
    TerminalState,
};
use krad_domain::agents_api::{session_infos, SessionInfo};
use krad_domain::{
    AgentPreset, AgentState, AskAnswer, AskKind, EnvironmentData, EnvironmentName, TerminalId,
    ToolDecision,
};
use krad_protocol::{
    AgentRef, AskSettlement, BusCommand, Command, EnvironmentCommand, ErrorCode, EventKind, Feed,
    PendingAsk, Reply, StatusReport,
};
use krad_runtime::{BusSetup, EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const CALLER: u64 = 1;
const ASKED_TOOL: &str = "Bash";
const ASKED_COMMAND: &str = "rm -rf build";
const ASK_STILL_WAITING: Duration = Duration::from_millis(300);
const NO_BUS: &str = "the runtime serves no bus";
const FEED_ENDED: &str = "the feed ended";
const NO_DELEGATION: &str = "no request was delegated";
const OPENED_FILE: &str = "src/lib.rs";

fn with_bus() -> Result<RuntimeHandle, Box<dyn Error>> {
    Ok(Runtime::start(RuntimeSetup {
        bus: Some(BusSetup::default()),
        ..RuntimeSetup::default()
    })?)
}

fn caller() -> TerminalHandle {
    TerminalHandle {
        environment: ENVIRONMENT.to_owned(),
        terminal: CALLER,
    }
}

fn caller_ref() -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        terminal: TerminalId(CALLER),
    }
}

fn call_in_background(
    runtime: &RuntimeHandle,
    request: Request,
) -> Result<JoinHandle<Result<Response, AgentsError>>, Box<dyn Error>> {
    let endpoint = &runtime.bus().ok_or(NO_BUS)?.endpoint;
    let address = endpoint.address.to_string();
    let token = endpoint.token.clone();
    Ok(thread::spawn(move || {
        call(&address, &token, &Caller::Terminal(caller()), request)
    }))
}

fn next_event(events: &mut EventReceiver) -> Result<EventKind, Box<dyn Error>> {
    loop {
        match block_on(events.next()).ok_or(FEED_ENDED)? {
            Feed::Event(event) => return Ok(event.kind),
            Feed::Snapshot(_) => {}
        }
    }
}

fn answer(
    runtime: &RuntimeHandle,
    pending: &PendingAsk,
    decision: &ToolDecision,
) -> Result<Reply, Box<dyn Error>> {
    let command = Command::Bus(BusCommand::AnswerAsk {
        ask: pending.ask,
        decision: decision.clone(),
    });
    Ok(block_on(runtime.send(runtime.next_request(), command))?)
}

#[test]
fn a_bus_ask_waits_for_an_answer_and_the_first_answer_wins() -> TestResult {
    let runtime = with_bus()?;
    let mut events = runtime.subscribe();
    let asked = call_in_background(
        &runtime,
        Request::Ask {
            tool: ASKED_TOOL.to_owned(),
            input: serde_json::json!({ "command": ASKED_COMMAND }),
            suggestions: Vec::new(),
            call: None,
        },
    )?;
    let EventKind::AskPending(pending) = next_event(&mut events)? else {
        return Err("the ask was not reported".into());
    };
    assert_eq!(pending.agent, caller_ref());
    assert_eq!(pending.request.tool, ASKED_TOOL);
    thread::sleep(ASK_STILL_WAITING);
    assert!(!asked.is_finished(), "the caller waits for the answer");

    let allow = ToolDecision::Allow {
        updated_input: None,
        permissions: Vec::new(),
    };
    assert_eq!(answer(&runtime, &pending, &allow)?, Reply::Accepted);
    let deny = ToolDecision::Deny {
        message: "late".to_owned(),
        interrupt: false,
    };
    assert!(matches!(
        answer(&runtime, &pending, &deny)?,
        Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            ..
        }
    ));
    assert_eq!(
        asked.join().map_err(|_| "the caller panicked")??,
        Response::Decided(allow)
    );
    assert_eq!(
        next_event(&mut events)?,
        EventKind::AskSettled {
            ask: pending.ask,
            settlement: AskSettlement::Answered,
        }
    );
    Ok(())
}

#[test]
fn an_ask_answered_always_hands_its_caller_the_suggested_rules() -> TestResult {
    let runtime = with_bus()?;
    let mut events = runtime.subscribe();
    let rule = serde_json::json!({ "type": "addRules" });
    let asked = call_in_background(
        &runtime,
        Request::Ask {
            tool: ASKED_TOOL.to_owned(),
            input: serde_json::json!({ "command": ASKED_COMMAND }),
            suggestions: vec![rule.clone()],
            call: None,
        },
    )?;
    let EventKind::AskPending(pending) = next_event(&mut events)? else {
        return Err("the ask was not reported".into());
    };
    assert!(pending.can_allow_always);
    assert!(matches!(
        &pending.kind,
        AskKind::Permission { tool, summary, .. } if tool == ASKED_TOOL && summary == ASKED_COMMAND
    ));
    let always = Command::Bus(BusCommand::Answer {
        ask: pending.ask,
        answer: AskAnswer::AllowAlways,
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), always))?,
        Reply::Accepted
    );
    assert_eq!(
        asked.join().map_err(|_| "the caller panicked")??,
        Response::Decided(ToolDecision::Allow {
            updated_input: None,
            permissions: vec![rule],
        })
    );
    Ok(())
}

#[test]
fn a_status_line_report_becomes_an_agent_status_event_and_stays_in_the_snapshot() -> TestResult {
    let runtime = with_bus()?;
    let mut events = runtime.subscribe();
    let status = AgentStatus {
        model_name: Some("Opus".to_owned()),
        context_used: Some(42),
        ..AgentStatus::default()
    };
    let reported = call_in_background(&runtime, Request::Status(status.clone()))?;
    assert_eq!(
        reported.join().map_err(|_| "the caller panicked")??,
        Response::Noted
    );
    let report = StatusReport {
        agent: caller_ref(),
        status,
    };
    assert_eq!(
        next_event(&mut events)?,
        EventKind::AgentStatus(report.clone())
    );
    let snapshot = block_on(runtime.snapshot(krad_protocol::Scope::Runtime))?;
    assert_eq!(snapshot.bus.statuses, vec![report]);
    Ok(())
}

#[test]
fn a_request_on_ui_state_is_delegated_and_its_answer_reaches_the_caller() -> TestResult {
    let runtime = with_bus()?;
    let mut delegations = runtime.delegate_bus();
    let opened = Request::OpenFile {
        path: PathBuf::from(OPENED_FILE),
        position: None,
    };
    let asked = call_in_background(&runtime, opened.clone())?;
    let delegated = block_on(delegations.next()).ok_or(NO_DELEGATION)?;
    assert_eq!(delegated.from, Caller::Terminal(caller()));
    assert_eq!(delegated.request, opened);
    assert!(!delegated.reply.is_canceled());
    let answer = Response::Opened {
        path: PathBuf::from(OPENED_FILE),
    };
    delegated.reply.send(answer.clone())?;
    assert_eq!(asked.join().map_err(|_| "the caller panicked")??, answer);
    Ok(())
}

#[test]
fn agents_list_the_directory_the_app_reports() -> TestResult {
    let runtime = with_bus()?;
    let agent = AgentInfo {
        environment: Some(ENVIRONMENT.to_owned()),
        terminal: CALLER,
        title: "Codex · 1".to_owned(),
        group: None,
        state: TerminalState::Running(AgentState::Idle),
        harness: Some(AgentPreset::Codex),
        is_self: false,
    };
    let observed = block_on(runtime.send(
        runtime.next_request(),
        Command::Bus(BusCommand::ObserveAgent {
            agent: AgentRef {
                environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
                terminal: TerminalId(CALLER),
            },
            seen: Some(agent.clone()),
            hint: None,
        }),
    ))?;
    assert_eq!(observed, Reply::Accepted);
    let listed = call_in_background(&runtime, Request::List)?;
    assert_eq!(
        listed.join().map_err(|_| "the caller panicked")??,
        Response::Agents(vec![AgentInfo {
            is_self: true,
            ..agent
        }])
    );
    Ok(())
}

fn ask(runtime: &RuntimeHandle, request: Request) -> Result<Response, Box<dyn Error>> {
    Ok(call_in_background(runtime, request)?
        .join()
        .map_err(|_| "the caller panicked")??)
}

fn listed(runtime: &RuntimeHandle) -> Result<Vec<SessionInfo>, Box<dyn Error>> {
    match ask(runtime, Request::Sessions)? {
        Response::Sessions(sessions) => Ok(sessions),
        other => Err(format!("sessions were not listed: {other:?}").into()),
    }
}

#[test]
fn sessions_are_listed_created_and_closed_by_the_runtime_without_the_app() -> TestResult {
    let runtime = with_bus()?;
    let mut environment = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = environment.add_group(PathBuf::from("/work/api"), None);
    environment
        .nodes
        .push(serde_json::from_value(serde_json::json!({
            "id": CALLER, "title": "shell", "state": "Idle", "group": group.0
        }))?);
    environment.next_id = CALLER + 1;
    environment.enter_sessions();
    let open = Command::Environments(EnvironmentCommand::Open {
        environment: Box::new(environment.clone()),
    });
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), open))?,
        Reply::Accepted
    );
    assert_eq!(listed(&runtime)?, session_infos(&environment));
    let created = ask(
        &runtime,
        Request::CreateSession {
            name: Some("review".to_owned()),
            group: None,
        },
    )?;
    let Response::SessionCreated { session } = created else {
        return Err(format!("no session was created: {created:?}").into());
    };
    let review = listed(&runtime)?
        .into_iter()
        .find(|listed| listed.session == session)
        .ok_or("the new session is listed")?;
    assert_eq!(review.name, "review");
    assert!(review.group.is_some(), "it lands in the caller's group");
    assert_eq!(
        ask(&runtime, Request::CloseSession { session })?,
        Response::SessionClosed { session }
    );
    assert!(listed(&runtime)?
        .iter()
        .all(|listed| listed.session != session));
    Ok(())
}
