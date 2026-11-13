use futures::executor::block_on;
use futures::StreamExt as _;
use krad_agents::{
    call, AgentsError, Caller, ErrorCode, LeadHandle, Request, Response, TerminalHandle,
    TerminalState,
};
use krad_domain::{
    AgentPreset, EnvironmentData, EnvironmentName, RuntimeState, SessionId, SessionLead,
    TerminalId, TerminalNode, WorkerSpawn,
};
use krad_runtime::{BusSetup, EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const LEAD: TerminalId = TerminalId(1);
const MEMBER: TerminalId = TerminalId(2);
const OPEN_WORKER: TerminalId = TerminalId(3);
const CLOSED_WORKER: TerminalId = TerminalId(4);
const NO_BUS: &str = "the runtime serves no bus";
const NO_DELEGATION: &str = "no request was delegated";
const NO_SESSION: &str = "sessions mode opened no session";
const CALLER_PANICKED: &str = "the caller panicked";

fn node(id: TerminalId) -> Result<TerminalNode, serde_json::Error> {
    serde_json::from_value(serde_json::json!({"id": id.0, "title": "agent", "state": "Idle"}))
}

fn led_environment() -> Result<(EnvironmentData, SessionId), Box<dyn Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = data.add_group(PathBuf::from("/work/api"), None);
    data.nodes = [LEAD, MEMBER, OPEN_WORKER, CLOSED_WORKER]
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
    for worker in [OPEN_WORKER, CLOSED_WORKER] {
        let spawn = WorkerSpawn {
            lead: Some(session),
            task: "Fix the webhook retries".to_owned(),
        };
        data.open_worker_session(worker, spawn);
    }
    data.retire_worker(CLOSED_WORKER);
    Ok((data, session))
}

fn start() -> Result<(RuntimeHandle, SessionId), Box<dyn Error>> {
    let (data, session) = led_environment()?;
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
    Ok((runtime, session))
}

fn call_as(
    runtime: &RuntimeHandle,
    caller: Caller,
    request: Request,
) -> Result<JoinHandle<Result<Response, AgentsError>>, Box<dyn Error>> {
    let endpoint = &runtime.bus().ok_or(NO_BUS)?.endpoint;
    let address = endpoint.address.to_string();
    let token = endpoint.token.clone();
    Ok(thread::spawn(move || {
        call(&address, &token, &caller, request)
    }))
}

fn call_from(
    runtime: &RuntimeHandle,
    terminal: TerminalId,
    request: Request,
) -> Result<JoinHandle<Result<Response, AgentsError>>, Box<dyn Error>> {
    let caller = Caller::Terminal(TerminalHandle {
        environment: ENVIRONMENT.to_owned(),
        terminal: terminal.0,
    });
    call_as(runtime, caller, request)
}

fn answered(asked: JoinHandle<Result<Response, AgentsError>>) -> Result<Response, Box<dyn Error>> {
    Ok(asked.join().map_err(|_| CALLER_PANICKED)??)
}

#[test]
fn a_leads_orders_the_runtime_cannot_carry_reach_the_app_as_the_lead_and_anyone_elses_are_refused(
) -> TestResult {
    let (runtime, session) = start()?;
    let mut delegations = runtime.delegate_bus();
    let spawn = Request::Spawn {
        agent: AgentPreset::ClaudeCode,
        worktree: Some("write-the-tests".to_owned()),
        task: "Write the tests".to_owned(),
    };
    let refused = answered(call_from(&runtime, MEMBER, spawn.clone())?)?;
    assert!(matches!(
        refused,
        Response::Error(error) if error.code == ErrorCode::Forbidden
    ));
    let asked = call_from(&runtime, LEAD, spawn.clone())?;
    let delegated = block_on(delegations.next()).ok_or(NO_DELEGATION)?;
    assert_eq!(
        delegated.from,
        Caller::Lead(LeadHandle {
            environment: ENVIRONMENT.to_owned(),
            session: session.as_u64(),
        })
    );
    assert_eq!(delegated.request, spawn);
    delegated.reply.send(Response::Spawned { terminal: 9 })?;
    assert_eq!(answered(asked)?, Response::Spawned { terminal: 9 });
    let follows = call_from(
        &runtime,
        LEAD,
        Request::Read {
            target: OPEN_WORKER.0,
            lines: 5,
        },
    )?;
    let delegated = block_on(delegations.next()).ok_or(NO_DELEGATION)?;
    assert!(matches!(delegated.from, Caller::Lead(_)));
    delegated.reply.send(Response::Screen(String::new()))?;
    answered(follows)?;
    Ok(())
}

#[test]
fn a_closed_worker_is_settled_and_retired_without_asking_the_app() -> TestResult {
    let (runtime, _) = start()?;
    let waited = answered(call_from(
        &runtime,
        MEMBER,
        Request::Wait {
            target: CLOSED_WORKER.0,
            timeout_secs: 1,
        },
    )?)?;
    assert_eq!(
        waited,
        Response::Settled {
            target: CLOSED_WORKER.0,
            state: TerminalState::Closed,
        }
    );
    let read = answered(call_from(
        &runtime,
        LEAD,
        Request::Read {
            target: CLOSED_WORKER.0,
            lines: 5,
        },
    )?)?;
    assert!(matches!(
        read,
        Response::Error(error) if error.code == ErrorCode::NotFound
    ));
    Ok(())
}

#[path = "lead/worker.rs"]
mod worker;
