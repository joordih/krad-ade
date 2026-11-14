use futures::executor::block_on;
use futures::StreamExt as _;
use krad_client::RuntimeClient;
use krad_domain::{
    AttentionKind, AttentionTarget, CloudRunnerId, EnvironmentData, EnvironmentName, Linger,
    RunnerState, RuntimeId, SessionSpec, TerminalId, Timestamp,
};
use krad_protocol::{
    AgentRef, AskId, AskSettlement, AttentionSnapshot, AttentionWaiting, BusSnapshot, Event,
    EventKind, Feed, GatewayState, HealthSnapshot, ModelsSnapshot, OpencodeSnapshot,
    RemoteBusState, RunnerBusShown, RunnerShown, RunnersSnapshot, Scope, Seq, Snapshot,
    UsageSnapshot,
};
use std::sync::Arc;

use super::remotes::Reached;
use super::{Area, Heard, News, RunnerReach, ViewModel};
use crate::fake_client::RecordedClient;

const MAIN: &str = "main";
const SIDE: &str = "side";
pub(super) const RUNNER: u64 = 5;
const UNREACHABLE: &str = "the runner's bus is out of reach";
pub(super) const RUNNER_ENVIRONMENT: &str = "runner";
const RUNNER_BUILD: &str = "0.9.0";

fn name(raw: &str) -> EnvironmentName {
    EnvironmentName::new(raw.to_owned())
}

fn target(terminal: u64) -> AttentionTarget {
    AttentionTarget::new(name(MAIN), TerminalId(terminal))
}

fn waiting(terminal: u64) -> AttentionWaiting {
    AttentionWaiting {
        target: target(terminal),
        kind: AttentionKind::Permission,
        since: Timestamp::now(),
    }
}

pub(super) fn snapshot() -> Snapshot {
    Snapshot {
        scope: Scope::Runtime,
        seq: Seq(10),
        bus: BusSnapshot::default(),
        attention: AttentionSnapshot {
            waiting: vec![waiting(1), waiting(2)],
            history: Vec::new(),
        },
        health: HealthSnapshot::default(),
        usage: UsageSnapshot::default(),
        models: ModelsSnapshot::default(),
        environments: vec![
            EnvironmentData::empty(name(MAIN)),
            EnvironmentData::empty(name(SIDE)),
        ],
        terminals: Vec::new(),
        hosts: Box::default(),
        chats: Vec::new(),
        opencode: OpencodeSnapshot::default(),
        gateway: GatewayState::default(),
        settings: Box::default(),
        git: Vec::new(),
        kraddies: Box::default(),
        runners: Box::new(RunnersSnapshot {
            buses: vec![RunnerBusShown {
                runner: CloudRunnerId(RUNNER),
                bus: RemoteBusState::Preparing,
            }],
            ..RunnersSnapshot::default()
        }),
        hosted: Vec::new(),
    }
}

fn event(seq: u64, kind: EventKind) -> Feed {
    Feed::Event(Event {
        seq: Seq(seq),
        kind,
    })
}

fn closed() -> EventKind {
    EventKind::ChatClosed {
        chat: AgentRef {
            environment: name(MAIN),
            terminal: TerminalId(1),
        },
    }
}

fn recorded() -> Vec<Feed> {
    vec![
        Feed::Snapshot(snapshot()),
        event(11, EventKind::AttentionLeft { target: target(1) }),
        event(
            12,
            EventKind::RunnerBusChanged(RunnerBusShown {
                runner: CloudRunnerId(RUNNER),
                bus: RemoteBusState::Unavailable {
                    reason: UNREACHABLE.to_owned(),
                },
            }),
        ),
        event(
            13,
            EventKind::EnvironmentsOrdered {
                order: vec![name(SIDE), name(MAIN)],
            },
        ),
        event(14, closed()),
    ]
}

fn followed(client: &RecordedClient, since: Option<Seq>) -> (ViewModel, News) {
    let mut model = ViewModel::default();
    let mut feeds = client.subscribe(since);
    while let Some(feed) = block_on(feeds.next()) {
        model.feed(feed);
    }
    let news = model.take_news();
    (model, news)
}

fn events(news: News) -> Vec<EventKind> {
    news.heard
        .into_iter()
        .filter_map(|heard| match heard {
            Heard::Event(kind) => Some(kind),
            _ => None,
        })
        .collect()
}

#[test]
fn recorded_events_fold_into_the_read_copy_after_its_snapshot() {
    let client = RecordedClient::new(recorded());
    let (model, news) = followed(&client, None);
    assert!(!model.attention.queue.contains(&target(1)));
    assert!(model.attention.queue.contains(&target(2)));
    assert_eq!(
        model.runner_buses.get(&CloudRunnerId(RUNNER)),
        Some(&RemoteBusState::Unavailable {
            reason: UNREACHABLE.to_owned()
        })
    );
    assert!(matches!(news.heard.first(), Some(Heard::Snapshot { .. })));
    assert_eq!(
        events(news),
        [
            EventKind::EnvironmentsOrdered {
                order: vec![name(SIDE), name(MAIN)],
            },
            closed()
        ],
        "what the view model does not keep goes to the shell"
    );
}

#[test]
fn a_view_model_resumed_after_its_snapshot_only_folds_the_later_events() {
    let client = RecordedClient::new(recorded());
    let (mut model, _) = followed(&client, None);
    model.runner_buses.clear();
    let mut feeds = client.subscribe(Some(Seq(12)));
    while let Some(feed) = block_on(feeds.next()) {
        model.feed(feed);
    }
    assert!(
        model.runner_buses.is_empty(),
        "the bus change came before the resume point"
    );
    let news = model.take_news();
    assert!(news.areas.is_empty());
    assert_eq!(events(news).len(), 2);
}

#[test]
fn a_folded_event_names_its_area_and_hands_its_side_effects_over_once() {
    let mut model = ViewModel::default();
    model.fold_event(EventKind::AttentionLeft { target: target(1) });
    model.fold_event(EventKind::AskSettled {
        ask: AskId(1),
        settlement: AskSettlement::Answered,
    });
    let news = model.take_news();
    assert_eq!(
        news.areas.into_iter().collect::<Vec<_>>(),
        [Area::Attention]
    );
    assert!(news.heard.is_empty(), "an ask never held is not let go");
    assert!(model.take_news().areas.is_empty());
}

pub(super) fn runner_snapshot() -> Snapshot {
    let mut environment = EnvironmentData::empty(name(RUNNER_ENVIRONMENT));
    environment.enter_sessions();
    environment.new_session(None, SessionSpec::default());
    Snapshot {
        environments: vec![environment],
        ..snapshot()
    }
}

pub(super) fn runner_moved(model: &mut ViewModel, state: RunnerState) {
    model.fold_event(EventKind::RunnerChanged(RunnerShown {
        runner: CloudRunnerId(RUNNER),
        state,
    }));
}

pub(super) fn ready() -> RunnerState {
    RunnerState::Ready {
        linger: Linger::Enabled,
    }
}

#[test]
fn a_reached_runner_s_sessions_come_from_its_own_feed_tagged_with_its_runtime() {
    let runner = CloudRunnerId(RUNNER);
    let mut model = ViewModel::default();
    runner_moved(&mut model, ready());
    let connection = model.remotes.reaching(runner);
    let client = RecordedClient::new(vec![Feed::Snapshot(runner_snapshot())]);
    let follow = model.adopt_reached(
        runner,
        connection,
        Reached::Connected {
            client: client.clone(),
            build: RUNNER_BUILD.to_owned(),
            stop: None,
        },
    );
    assert!(follow.is_some());
    assert_eq!(
        model.remotes.get(runner).map(|remote| remote.reach.clone()),
        Some(RunnerReach::Connected {
            build: RUNNER_BUILD.to_owned()
        })
    );
    let mut feeds = client.subscribe(None);
    while let Some(feed) = block_on(feeds.next()) {
        assert_eq!(model.fold_remote(runner, connection, feed), Some(true));
    }
    let sessions: Vec<(RuntimeId, &str)> = model
        .remotes
        .sessions()
        .map(|(runtime, environment, _)| (runtime, environment.as_str()))
        .collect();
    assert_eq!(sessions, [(RuntimeId::Runner(runner), RUNNER_ENVIRONMENT)]);
    assert!(
        model.attention.queue.is_empty(),
        "a runner's feed stays out of the local copy"
    );
    assert_eq!(
        model.fold_remote(runner, connection + 1, Feed::Snapshot(snapshot())),
        None,
        "a feed of an older connection is not folded"
    );
}

#[test]
fn an_older_runner_keeps_the_process_wire_until_it_leaves_ready() {
    let runner = CloudRunnerId(RUNNER);
    let mut model = ViewModel::default();
    runner_moved(&mut model, ready());
    let connection = model.remotes.reaching(runner);
    let follow = model.adopt_reached(runner, connection, Reached::ProcessWire);
    assert!(follow.is_none());
    assert_eq!(
        model.remotes.get(runner).map(|remote| remote.reach.clone()),
        Some(RunnerReach::ProcessWire)
    );
    runner_moved(&mut model, RunnerState::NotConnected);
    assert!(model.remotes.get(runner).is_none());
    let unreached: Arc<RecordedClient> = RecordedClient::new(Vec::new());
    let late = model.adopt_reached(
        runner,
        connection,
        Reached::Connected {
            client: unreached,
            build: RUNNER_BUILD.to_owned(),
            stop: None,
        },
    );
    assert!(
        late.is_none() && model.remotes.get(runner).is_none(),
        "a connection that finishes after its runner left is dropped"
    );
}

#[test]
fn acp_adapters_and_listings_are_read_from_the_runtime_s_events_in_the_view_model() {
    use super::super::super::harness_models::AcpListing;
    use krad_domain::{AcpAgentId, Host, ManagedAdapter};
    use krad_protocol::{AcpListing as Listing, AcpOffers, KeptAdapter};
    let Ok(agent) = "gemini".parse::<AcpAgentId>() else {
        unreachable!("a valid agent id");
    };
    let mut model = ViewModel::default();
    model.fold_event(EventKind::AcpAdapterChanged(KeptAdapter {
        agent,
        adapter: ManagedAdapter::Installing,
    }));
    assert_eq!(
        model.adapters.get(&agent),
        Some(&ManagedAdapter::Installing)
    );
    model.fold_event(EventKind::AcpOffers(AcpOffers {
        host: Host::Local,
        agent,
        listing: Listing::Failed {
            error: "no node".to_owned(),
        },
    }));
    assert_eq!(
        model.acp.acp_listing(&Host::Local, agent),
        Some(&AcpListing::Failed("no node".to_owned()))
    );
    let heard = model.take_news().heard;
    assert_eq!(heard.len(), 2, "the shell still hears both to react");
}
