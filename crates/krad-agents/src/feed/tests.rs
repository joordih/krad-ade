use super::*;
use crate::protocol::{AgentFilter, CoarseState};
use krad_domain::{AgentPreset, AgentState};
use std::time::{Duration, UNIX_EPOCH};

const MAIN: &str = "main";
const SIDE: &str = "side";

fn at(second: u64) -> Timestamp {
    Timestamp::from(UNIX_EPOCH + Duration::from_secs(second))
}

fn running(state: AgentState) -> TerminalState {
    TerminalState::Running(state)
}

fn agent(terminal: u64, state: TerminalState) -> AgentInfo {
    AgentInfo {
        environment: Some(MAIN.to_owned()),
        terminal,
        title: format!("Codex · {terminal}"),
        group: Some("api".to_owned()),
        state,
        harness: Some(AgentPreset::Codex),
        is_self: true,
    }
}

fn key(terminal: u64) -> AgentKey {
    AgentKey::new(MAIN, terminal)
}

fn events(lines: &[Response]) -> Vec<&AgentEvent> {
    lines
        .iter()
        .filter_map(|line| match line {
            Response::AgentEvent(event) => Some(&**event),
            _ => None,
        })
        .collect()
}

fn drain(subscription: &Subscription) -> Vec<Response> {
    subscription.lines.try_iter().collect()
}

#[test]
fn a_change_is_an_opening_a_state_change_or_a_closing() {
    let idle = agent(1, running(AgentState::Idle));
    let working = agent(1, running(AgentState::Working));
    assert_eq!(change(None, None), None);
    assert_eq!(
        change(None, Some(&idle)).map(|change| (change.kind, change.previous)),
        Some((AgentChangeKind::Opened, None))
    );
    assert_eq!(change(Some(&idle), Some(&idle)), None);
    let renamed = AgentInfo {
        title: "Renamed".to_owned(),
        ..idle.clone()
    };
    assert_eq!(change(Some(&idle), Some(&renamed)), None);
    assert_eq!(
        change(Some(&idle), Some(&working)),
        Some(Change {
            kind: AgentChangeKind::StateChanged,
            previous: Some(running(AgentState::Idle)),
            state: running(AgentState::Working),
        })
    );
    assert_eq!(
        change(Some(&working), None),
        Some(Change {
            kind: AgentChangeKind::Closed,
            previous: Some(running(AgentState::Working)),
            state: TerminalState::Closed,
        })
    );
}

#[test]
fn reasons_follow_the_transition_unless_the_app_names_one() {
    let working = running(AgentState::Working);
    let cases = [
        (None, working, None),
        (
            Some(running(AgentState::Idle)),
            working,
            Some(StateReason::TurnStarted),
        ),
        (
            Some(working),
            running(AgentState::Done),
            Some(StateReason::TurnFinished),
        ),
        (
            Some(working),
            running(AgentState::Idle),
            Some(StateReason::TurnFinished),
        ),
        (
            Some(running(AgentState::Done)),
            running(AgentState::Idle),
            None,
        ),
        (
            Some(working),
            running(AgentState::WaitingForInput),
            Some(StateReason::InputPending),
        ),
        (
            Some(working),
            TerminalState::Exited,
            Some(StateReason::ProcessExited),
        ),
        (Some(working), TerminalState::Closed, None),
    ];
    for (previous, state, reason) in cases {
        assert_eq!(
            StateReason::inferred(previous, state),
            reason,
            "{previous:?} -> {state}"
        );
    }
    let feed = AgentFeed::new();
    let watcher = feed.subscribe(AgentSubscription::default(), at(0));
    feed.observe(key(1), Some(agent(1, working)), None, at(1));
    let asked = agent(1, running(AgentState::WaitingForInput));
    feed.observe(
        key(1),
        Some(asked),
        Some(StateReason::PermissionPending),
        at(2),
    );
    let lines = drain(&watcher);
    let reasons: Vec<_> = events(&lines).iter().map(|event| event.reason).collect();
    assert_eq!(reasons, [None, Some(StateReason::PermissionPending)]);
}

#[test]
fn the_four_coarse_states_fold_done_and_exited() {
    let folded: Vec<CoarseState> = [
        running(AgentState::Idle),
        running(AgentState::Done),
        running(AgentState::Working),
        running(AgentState::WaitingForInput),
        TerminalState::Exited,
        TerminalState::Closed,
    ]
    .into_iter()
    .map(CoarseState::from)
    .collect();
    assert_eq!(
        folded,
        [
            CoarseState::Idle,
            CoarseState::Idle,
            CoarseState::Working,
            CoarseState::WaitingForInput,
            CoarseState::Closed,
            CoarseState::Closed,
        ]
    );
}

#[test]
fn a_subscriber_gets_a_snapshot_then_numbered_events_in_order() {
    let feed = AgentFeed::new();
    feed.observe(
        key(1),
        Some(agent(1, running(AgentState::Idle))),
        None,
        at(1),
    );
    let watcher = feed.subscribe(AgentSubscription::default(), at(2));
    let [Response::AgentSnapshot(snapshot)] = watcher.opening.as_slice() else {
        unreachable!("a fresh subscriber starts with a snapshot")
    };
    assert_eq!((snapshot.seq, snapshot.resync), (1, false));
    assert_eq!(snapshot.agents.len(), 1);
    assert_eq!(snapshot.agents[0].environment.as_deref(), Some(MAIN));
    assert!(!snapshot.agents[0].is_self);
    feed.observe(
        key(1),
        Some(agent(1, running(AgentState::Working))),
        None,
        at(3),
    );
    feed.observe(
        key(1),
        Some(agent(1, running(AgentState::Working))),
        None,
        at(4),
    );
    feed.observe(
        key(2),
        Some(agent(2, running(AgentState::Idle))),
        None,
        at(5),
    );
    feed.observe(key(1), None, Some(StateReason::UserClosed), at(6));
    let lines = drain(&watcher);
    let seen: Vec<_> = events(&lines)
        .iter()
        .map(|event| (event.seq, event.terminal, event.kind, event.state))
        .collect();
    assert_eq!(
        seen,
        [
            (
                2,
                1,
                AgentChangeKind::StateChanged,
                running(AgentState::Working)
            ),
            (3, 2, AgentChangeKind::Opened, running(AgentState::Idle)),
            (4, 1, AgentChangeKind::Closed, TerminalState::Closed),
        ]
    );
    let closed = events(&lines)[2];
    assert_eq!(closed.title, "Codex · 1");
    assert_eq!(closed.reason, Some(StateReason::UserClosed));
    assert_eq!(closed.previous, Some(running(AgentState::Working)));
    assert_eq!(feed.heartbeat(at(7)).seq, 4);
}

#[test]
fn a_resume_inside_the_ring_replays_and_outside_it_resyncs() {
    let feed = AgentFeed::new();
    let first = feed.subscribe(AgentSubscription::default(), at(0));
    let Response::AgentSnapshot(snapshot) = &first.opening[0] else {
        unreachable!("a fresh subscriber starts with a snapshot")
    };
    let stream = snapshot.stream.clone();
    for (second, state) in [AgentState::Working, AgentState::Done, AgentState::Idle]
        .into_iter()
        .enumerate()
    {
        feed.observe(
            key(1),
            Some(agent(1, running(state))),
            None,
            at(second as u64),
        );
    }
    let resumed = feed.subscribe(
        AgentSubscription {
            since_seq: Some(1),
            stream: Some(stream.clone()),
            ..AgentSubscription::default()
        },
        at(9),
    );
    assert!(matches!(resumed.opening[0], Response::Heartbeat(_)));
    let replayed: Vec<u64> = events(&resumed.opening)
        .iter()
        .map(|event| event.seq)
        .collect();
    assert_eq!(replayed, [2, 3]);
    let up_to_date = feed.subscribe(
        AgentSubscription {
            since_seq: Some(3),
            stream: Some(stream.clone()),
            ..AgentSubscription::default()
        },
        at(9),
    );
    assert_eq!(up_to_date.opening.len(), 1);
    assert!(matches!(up_to_date.opening[0], Response::Heartbeat(_)));
    for (since, stream) in [
        (Some(4), Some(stream.clone())),
        (Some(1), None),
        (Some(1), Some(StreamId("another run".to_owned()))),
    ] {
        let resynced = feed.subscribe(
            AgentSubscription {
                since_seq: since,
                stream,
                ..AgentSubscription::default()
            },
            at(9),
        );
        assert!(
            matches!(&resynced.opening[..], [Response::AgentSnapshot(snapshot)] if snapshot.resync)
        );
    }
    for second in 0..EVENT_RING_CAPACITY as u64 {
        let state = if second % 2 == 0 {
            AgentState::Working
        } else {
            AgentState::Idle
        };
        feed.observe(key(1), Some(agent(1, running(state))), None, at(second));
    }
    let lost = feed.subscribe(
        AgentSubscription {
            since_seq: Some(1),
            stream: Some(stream),
            ..AgentSubscription::default()
        },
        at(9),
    );
    assert!(matches!(&lost.opening[..], [Response::AgentSnapshot(snapshot)] if snapshot.resync));
}

#[test]
fn a_filter_picks_by_environment_group_harness_and_terminal() {
    let codex = agent(1, running(AgentState::Idle));
    let claude = AgentInfo {
        terminal: 2,
        group: None,
        harness: Some(AgentPreset::ClaudeCode),
        ..codex.clone()
    };
    let filters = [
        (AgentFilter::default(), [true, true]),
        (
            AgentFilter {
                group: Some("api".to_owned()),
                ..AgentFilter::default()
            },
            [true, false],
        ),
        (
            AgentFilter {
                harness: Some(AgentPreset::ClaudeCode),
                ..AgentFilter::default()
            },
            [false, true],
        ),
        (
            AgentFilter {
                terminals: vec![2, 9],
                ..AgentFilter::default()
            },
            [false, true],
        ),
        (
            AgentFilter {
                environment: Some(SIDE.to_owned()),
                ..AgentFilter::default()
            },
            [false, false],
        ),
    ];
    for (filter, wanted) in filters {
        assert_eq!(
            [filter.admits_agent(&codex), filter.admits_agent(&claude)],
            wanted,
            "{filter:?}"
        );
    }
    let feed = AgentFeed::new();
    let only_claude = feed.subscribe(
        AgentSubscription {
            filter: AgentFilter {
                harness: Some(AgentPreset::ClaudeCode),
                ..AgentFilter::default()
            },
            ..AgentSubscription::default()
        },
        at(0),
    );
    feed.observe(key(1), Some(codex), None, at(1));
    feed.observe(key(2), Some(claude), None, at(2));
    let lines = drain(&only_claude);
    let terminals: Vec<u64> = events(&lines).iter().map(|event| event.terminal).collect();
    assert_eq!(terminals, [2]);
}

#[test]
fn a_slow_subscriber_is_dropped_without_holding_up_the_others() {
    let feed = AgentFeed::new();
    let slow = feed.subscribe(AgentSubscription::default(), at(0));
    let quick = feed.subscribe(AgentSubscription::default(), at(0));
    for round in 0..=SUBSCRIBER_QUEUE as u64 {
        let state = if round % 2 == 0 {
            AgentState::Working
        } else {
            AgentState::Idle
        };
        feed.observe(key(1), Some(agent(1, running(state))), None, at(round));
        drain(&quick);
    }
    assert!(slow.lagged.load(Ordering::Acquire));
    assert!(!quick.lagged.load(Ordering::Acquire));
    assert_eq!(drain(&slow).len(), SUBSCRIBER_QUEUE);
    assert!(matches!(
        slow.lines.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
    feed.observe(key(1), None, None, at(999));
    assert_eq!(drain(&quick).len(), 1);
    feed.unsubscribe(quick.id);
    feed.observe(
        key(2),
        Some(agent(2, running(AgentState::Idle))),
        None,
        at(1000),
    );
    assert!(matches!(
        quick.lines.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
}

#[test]
fn reconciling_opens_new_agents_and_closes_vanished_ones() {
    let feed = AgentFeed::new();
    let watcher = feed.subscribe(AgentSubscription::default(), at(0));
    let side = AgentInfo {
        environment: Some(SIDE.to_owned()),
        ..agent(1, running(AgentState::Idle))
    };
    let environments = [MAIN.to_owned(), SIDE.to_owned()];
    feed.reconcile(
        &environments,
        vec![
            agent(1, running(AgentState::Idle)),
            agent(2, running(AgentState::Working)),
            side.clone(),
        ],
        at(1),
    );
    feed.reconcile(
        &environments,
        vec![agent(1, running(AgentState::Idle)), side.clone()],
        at(2),
    );
    feed.reconcile(
        &environments[..1],
        vec![agent(1, running(AgentState::Idle))],
        at(3),
    );
    let lines = drain(&watcher);
    let seen: Vec<_> = events(&lines)
        .iter()
        .map(|event| {
            (
                event.environment.as_str(),
                event.terminal,
                event.kind,
                event.reason,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (MAIN, 1, AgentChangeKind::Opened, None),
            (MAIN, 2, AgentChangeKind::Opened, None),
            (SIDE, 1, AgentChangeKind::Opened, None),
            (MAIN, 2, AgentChangeKind::Closed, None),
            (
                SIDE,
                1,
                AgentChangeKind::Closed,
                Some(StateReason::EnvironmentClosed)
            ),
        ]
    );
}
