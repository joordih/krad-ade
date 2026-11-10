use futures::channel::mpsc;
use futures::executor::block_on;
use futures::StreamExt;
use krad_agents::{
    call, lead_refusal, serve, watch, AgentChangeKind, AgentFeed, AgentInfo, AgentKey,
    AgentSubscription, AgentsError, ApiError, Caller, CloseOrder, CloseTarget, Closed,
    ClosedTarget, ErrorCode, Incoming, LeadHandle, Request, Response, StateReason, TerminalHandle,
    TerminalState, Timestamp, WhenWorking, WorktreeFate,
};
use krad_domain::{
    AgentPreset, AgentState, ChatBookmarks, ChatCheckpoints, ChatUsage, CloseRefusal, CloseSubject,
    EnvironmentData, EnvironmentName, GroupId, NodeKind, SessionId, SessionLead, TerminalId,
    TerminalNode, TextScale, WorkerSpawn,
};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

const CHANNEL_CAPACITY: usize = 8;
const ENVIRONMENT: &str = "main";
const LEAD_TAB: u64 = 1;
const IDLE_WORKER: u64 = 2;
const BUSY_WORKER: u64 = 3;
const FOREIGN_WORKER: u64 = 4;
const SECOND_IDLE_WORKER: u64 = 5;
const MISSING: u64 = 99;
const STREAM_WAIT: Duration = Duration::from_secs(5);
const PROMPT_SUBMITTED: &str = "UserPromptSubmit";
const STOPPED: &str = "Stop";

fn node(id: u64, group: GroupId, state: AgentState) -> TerminalNode {
    TerminalNode {
        id: TerminalId(id),
        title: format!("Codex · {id}"),
        state,
        harness: Some(AgentPreset::Codex),
        resume: None,
        group: Some(group),
        text_scale: TextScale::default(),
        kind: NodeKind::default(),
        file: None,
        extension: None,
        bookmarks: ChatBookmarks::default(),
        usage: ChatUsage::default(),
        checkpoints: ChatCheckpoints::default(),
        choice: krad_domain::ChatChoice::default(),
    }
}

fn make_lead(data: &mut EnvironmentData, session: SessionId, tab: Option<u64>) {
    if let Some(session) = data.sessions.iter_mut().find(|s| s.id == session) {
        let mut lead = SessionLead::new(AgentPreset::ClaudeCode, None);
        lead.terminal = tab.map(TerminalId);
        session.lead = Some(lead);
    }
}

fn environment() -> (EnvironmentData, SessionId) {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let api = data.add_group(PathBuf::from("/work/api"), None);
    data.nodes.push(node(LEAD_TAB, api, AgentState::Idle));
    for (id, state) in [
        (IDLE_WORKER, AgentState::Done),
        (BUSY_WORKER, AgentState::Working),
        (FOREIGN_WORKER, AgentState::Idle),
        (SECOND_IDLE_WORKER, AgentState::Idle),
    ] {
        data.nodes.push(node(id, api, state));
    }
    data.enter_sessions();
    let lead = data
        .active_session
        .unwrap_or_else(|| unreachable!("Sessions mode opens a session"));
    make_lead(&mut data, lead, Some(LEAD_TAB));
    let other = data.new_session(Some(api), krad_domain::SessionSpec::default());
    make_lead(&mut data, other, None);
    data.active_session = Some(lead);
    for (id, owner) in [
        (IDLE_WORKER, lead),
        (BUSY_WORKER, lead),
        (FOREIGN_WORKER, other),
        (SECOND_IDLE_WORKER, lead),
    ] {
        let spawn = WorkerSpawn {
            lead: Some(owner),
            task: format!("Task {id}"),
        };
        data.open_worker_session(TerminalId(id), spawn);
    }
    (data, lead)
}

fn refusal(refusal: CloseRefusal) -> Response {
    let code = match refusal {
        CloseRefusal::Unknown | CloseRefusal::AlreadyClosed => ErrorCode::NotFound,
        CloseRefusal::NotOwned => ErrorCode::NotOwned,
        CloseRefusal::Itself => ErrorCode::Forbidden,
        CloseRefusal::Working => ErrorCode::Busy,
    };
    Response::error(code, format!("{refusal:?}"))
}

fn publish_closed(feed: &AgentFeed, response: &Response) {
    for terminal in closed_ids(response) {
        feed.observe(
            AgentKey::new(ENVIRONMENT, terminal),
            None,
            Some(StateReason::UserClosed),
            Timestamp::now(),
        );
    }
}

fn apply_hook(data: &mut EnvironmentData, feed: &AgentFeed, from: &Caller, request: &Request) {
    let (
        Caller::Terminal(from),
        Request::Hook {
            harness,
            event,
            tool,
            ..
        },
    ) = (from, request)
    else {
        return;
    };
    let Some(signal) = harness.hook_signal(event, tool.as_deref()) else {
        return;
    };
    let Some(node) = data
        .nodes
        .iter_mut()
        .find(|node| node.id.as_u64() == from.terminal)
    else {
        return;
    };
    node.state = AgentState::from(signal);
    let seen = info(node);
    feed.observe(
        AgentKey::new(ENVIRONMENT, from.terminal),
        Some(seen),
        None,
        Timestamp::now(),
    );
}

fn info(node: &TerminalNode) -> AgentInfo {
    AgentInfo {
        environment: Some(ENVIRONMENT.to_owned()),
        terminal: node.id.as_u64(),
        title: node.title.clone(),
        group: None,
        state: TerminalState::Running(node.state),
        harness: node.harness,
        is_self: false,
    }
}

fn close(data: &mut EnvironmentData, lead: SessionId, order: CloseOrder) -> Response {
    let busy = |terminal: TerminalId| {
        data.nodes
            .iter()
            .any(|node| node.id == terminal && node.state == AgentState::Working)
    };
    let subjects = match order.target {
        CloseTarget::Agent(id) => vec![CloseSubject::Worker(TerminalId(id))],
        CloseTarget::Session(id) => vec![CloseSubject::Session(SessionId(id))],
        CloseTarget::AllIdle => data
            .idle_workers(lead, busy)
            .into_iter()
            .map(CloseSubject::Worker)
            .collect(),
    };
    let plans: Result<Vec<_>, _> = subjects
        .into_iter()
        .map(|subject| data.plan_close(lead, subject, order.when_working, busy))
        .collect();
    let plans = match plans {
        Ok(plans) => plans,
        Err(refused) => return refusal(refused),
    };
    let mut closed = Vec::new();
    for plan in plans {
        for terminal in &plan.terminals {
            data.drop_terminal(*terminal);
        }
        let CloseSubject::Worker(terminal) = plan.subject else {
            unreachable!("this fake only closes workers");
        };
        closed.push(Closed {
            target: ClosedTarget::Agent(terminal.as_u64()),
            worktree: None,
        });
    }
    Response::Closed(closed)
}

fn listed(data: &EnvironmentData, all: bool) -> Vec<AgentInfo> {
    let open = data.nodes.iter().map(|node| AgentInfo {
        environment: None,
        terminal: node.id.as_u64(),
        title: node.title.clone(),
        group: None,
        state: TerminalState::Running(node.state),
        harness: node.harness,
        is_self: false,
    });
    let closed = data
        .sessions
        .iter()
        .filter_map(|session| session.lead.as_ref())
        .flat_map(|lead| &lead.workers)
        .filter(|worker| all && worker.session.is_none())
        .map(|worker| AgentInfo {
            environment: None,
            terminal: worker.terminal.as_u64(),
            title: worker.task.clone(),
            group: None,
            state: TerminalState::Closed,
            harness: None,
            is_self: false,
        });
    open.chain(closed).collect()
}

fn fake_app(mut requests: mpsc::Receiver<Incoming>, feed: AgentFeed) -> std_mpsc::Receiver<()> {
    let (published, ready) = std_mpsc::channel();
    thread::spawn(move || {
        let (mut data, lead) = environment();
        let open = data.nodes.iter().map(info).collect();
        feed.reconcile(&[ENVIRONMENT.to_owned()], open, Timestamp::now());
        assert!(published.send(()).is_ok());
        while let Some(Incoming {
            from,
            request,
            reply,
        }) = block_on(requests.next())
        {
            let answer = if let Some(refused) = lead_refusal(&from, &request) {
                refused
            } else {
                match request {
                    Request::Close(order) => {
                        let closed = close(&mut data, lead, order);
                        publish_closed(&feed, &closed);
                        closed
                    }
                    hook @ Request::Hook { .. } => {
                        apply_hook(&mut data, &feed, &from, &hook);
                        Response::Noted
                    }
                    Request::List => Response::Agents(listed(&data, false)),
                    Request::ListAll => Response::Agents(listed(&data, true)),
                    _ => Response::error(ErrorCode::Unsupported, "not in this fake"),
                }
            };
            assert!(reply.send(answer).is_ok());
        }
    });
    ready
}

fn lead() -> Caller {
    Caller::Lead(LeadHandle {
        environment: ENVIRONMENT.to_owned(),
        session: 1,
    })
}

fn worker() -> Caller {
    Caller::Terminal(TerminalHandle {
        environment: ENVIRONMENT.to_owned(),
        terminal: IDLE_WORKER,
    })
}

fn order(target: CloseTarget, when_working: WhenWorking) -> Request {
    Request::Close(CloseOrder {
        target,
        when_working,
        worktree: WorktreeFate::Keep,
    })
}

fn code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error(ApiError { code, .. }) => Some(*code),
        _ => None,
    }
}

fn closed_ids(response: &Response) -> Vec<u64> {
    let Response::Closed(closed) = response else {
        return Vec::new();
    };
    closed
        .iter()
        .filter_map(|closed| match closed.target {
            ClosedTarget::Agent(id) => Some(id),
            ClosedTarget::Session(_) => None,
        })
        .collect()
}

#[test]
fn a_lead_closes_its_workers_through_the_bus_and_nothing_else() -> Result<(), AgentsError> {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    let ready = fake_app(receiver, endpoint.feed.clone());
    assert!(
        ready.recv_timeout(STREAM_WAIT).is_ok(),
        "the fake app publishes its agents"
    );
    let address = endpoint.address.to_string();
    let as_lead = |request| call(&address, &endpoint.lead_token, &lead(), request);
    let refuse = WhenWorking::Refuse;

    let by_worker = call(
        &address,
        &endpoint.token,
        &worker(),
        order(CloseTarget::Agent(IDLE_WORKER), refuse),
    )?;
    assert_eq!(code(&by_worker), Some(ErrorCode::Forbidden));

    for (target, expected) in [
        (MISSING, ErrorCode::NotFound),
        (FOREIGN_WORKER, ErrorCode::NotOwned),
        (LEAD_TAB, ErrorCode::Forbidden),
        (BUSY_WORKER, ErrorCode::Busy),
    ] {
        let answer = as_lead(order(CloseTarget::Agent(target), refuse))?;
        assert_eq!(code(&answer), Some(expected), "closing {target}");
    }

    let closed = as_lead(order(CloseTarget::Agent(IDLE_WORKER), refuse))?;
    assert_eq!(closed_ids(&closed), vec![IDLE_WORKER]);
    let again = as_lead(order(CloseTarget::Agent(IDLE_WORKER), refuse))?;
    assert_eq!(code(&again), Some(ErrorCode::NotFound));

    let all_idle = as_lead(order(CloseTarget::AllIdle, refuse))?;
    assert_eq!(closed_ids(&all_idle), vec![SECOND_IDLE_WORKER]);
    let forced = as_lead(order(
        CloseTarget::Agent(BUSY_WORKER),
        WhenWorking::CancelTurn,
    ))?;
    assert_eq!(closed_ids(&forced), vec![BUSY_WORKER]);

    let Response::Agents(open) = as_lead(Request::List)? else {
        unreachable!("the app lists its agents");
    };
    let open: Vec<u64> = open.iter().map(|agent| agent.terminal).collect();
    assert_eq!(open, vec![LEAD_TAB, FOREIGN_WORKER]);
    let Response::Agents(every) = as_lead(Request::ListAll)? else {
        unreachable!("the app lists its agents");
    };
    let closed: Vec<u64> = every
        .iter()
        .filter(|agent| agent.state == TerminalState::Closed)
        .map(|agent| agent.terminal)
        .collect();
    assert_eq!(closed, vec![IDLE_WORKER, BUSY_WORKER, SECOND_IDLE_WORKER]);
    Ok(())
}

fn hook(event: &str) -> Request {
    Request::Hook {
        harness: AgentPreset::ClaudeCode,
        event: event.to_owned(),
        tool: None,
        detail: None,
        resume: None,
        mode: None,
    }
}

#[test]
fn a_watcher_sees_the_snapshot_each_state_change_and_the_close() -> Result<(), AgentsError> {
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let endpoint = serve(sender)?;
    let ready = fake_app(receiver, endpoint.feed.clone());
    assert!(
        ready.recv_timeout(STREAM_WAIT).is_ok(),
        "the fake app publishes its agents"
    );
    let address = endpoint.address.to_string();
    let (lines, streamed) = std_mpsc::channel();
    let watching = {
        let (address, token) = (address.clone(), endpoint.token.clone());
        thread::spawn(move || {
            watch(
                &address,
                &token,
                &Caller::Client,
                AgentSubscription::default(),
                |line| {
                    let closed = matches!(&line, Response::AgentEvent(event) if event.kind == AgentChangeKind::Closed);
                    if lines.send(line).is_err() || closed {
                        return ControlFlow::Break(());
                    }
                    ControlFlow::Continue(())
                },
            )
        })
    };
    let Ok(Response::AgentSnapshot(snapshot)) = streamed.recv_timeout(STREAM_WAIT) else {
        unreachable!("a watcher starts with a snapshot");
    };
    assert_eq!(snapshot.agents.len(), 5);
    assert!(snapshot
        .agents
        .iter()
        .all(|agent| agent.environment.as_deref() == Some(ENVIRONMENT)));

    let as_worker = |request| call(&address, &endpoint.token, &worker(), request);
    assert_eq!(as_worker(hook(PROMPT_SUBMITTED))?, Response::Noted);
    assert_eq!(as_worker(hook(STOPPED))?, Response::Noted);
    let closed = call(
        &address,
        &endpoint.lead_token,
        &lead(),
        order(CloseTarget::Agent(IDLE_WORKER), WhenWorking::Refuse),
    )?;
    assert_eq!(closed_ids(&closed), vec![IDLE_WORKER]);

    let events: Vec<_> = std::iter::from_fn(|| streamed.recv_timeout(STREAM_WAIT).ok())
        .filter_map(|line| match line {
            Response::AgentEvent(event) => Some(event),
            _ => None,
        })
        .collect();
    let seen: Vec<_> = events
        .iter()
        .map(|event| {
            (
                event.terminal,
                event.kind,
                event.state.to_string(),
                event.reason,
            )
        })
        .collect();
    let changed = AgentChangeKind::StateChanged;
    assert_eq!(
        seen,
        [
            (
                IDLE_WORKER,
                changed,
                "working".to_owned(),
                Some(StateReason::TurnStarted)
            ),
            (
                IDLE_WORKER,
                changed,
                "done".to_owned(),
                Some(StateReason::TurnFinished)
            ),
            (
                IDLE_WORKER,
                AgentChangeKind::Closed,
                "closed".to_owned(),
                Some(StateReason::UserClosed)
            ),
        ]
    );
    let seqs: Vec<u64> = events.iter().map(|event| event.seq).collect();
    assert_eq!(seqs, [snapshot.seq + 1, snapshot.seq + 2, snapshot.seq + 3]);
    assert!(matches!(watching.join(), Ok(Ok(()))));
    Ok(())
}
