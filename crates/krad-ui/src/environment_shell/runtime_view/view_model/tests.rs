use futures::executor::block_on;
use futures::StreamExt as _;
use krad_client::RuntimeClient;
use krad_domain::{
    AttentionKind, AttentionTarget, CloudRunnerId, EnvironmentData, EnvironmentName, TerminalId,
    Timestamp,
};
use krad_protocol::{
    AgentRef, AttentionSnapshot, AttentionWaiting, BusSnapshot, Event, EventKind, Feed,
    GatewayState, HealthSnapshot, ModelsSnapshot, OpencodeSnapshot, RemoteBusState, RunnerBusShown,
    RunnersSnapshot, Scope, Seq, Snapshot, UsageSnapshot,
};

use super::ViewModel;
use crate::fake_client::RecordedClient;

const MAIN: &str = "main";
const SIDE: &str = "side";
const RUNNER: u64 = 5;
const UNREACHABLE: &str = "the runner's bus is out of reach";

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

fn snapshot() -> Snapshot {
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

fn followed(client: &RecordedClient, since: Option<Seq>) -> (ViewModel, Vec<EventKind>) {
    let mut model = ViewModel::default();
    let mut unfolded = Vec::new();
    let mut feeds = client.subscribe(since);
    while let Some(feed) = block_on(feeds.next()) {
        match feed {
            Feed::Snapshot(mut snapshot) => model.adopt_snapshot(&mut snapshot),
            Feed::Event(event) => unfolded.extend(model.fold_event(event.kind)),
        }
    }
    (model, unfolded)
}

#[test]
fn recorded_events_fold_into_the_read_copy_after_its_snapshot() {
    let client = RecordedClient::new(recorded());
    let (model, unfolded) = followed(&client, None);
    assert!(!model.attention.queue.contains(&target(1)));
    assert!(model.attention.queue.contains(&target(2)));
    assert_eq!(
        model.runner_buses.get(&CloudRunnerId(RUNNER)),
        Some(&RemoteBusState::Unavailable {
            reason: UNREACHABLE.to_owned()
        })
    );
    assert_eq!(model.environments.names(), [name(SIDE), name(MAIN)]);
    assert_eq!(
        unfolded,
        [closed()],
        "what the view model does not keep goes to the shell"
    );
}

#[test]
fn a_view_model_resumed_after_its_snapshot_only_folds_the_later_events() {
    let client = RecordedClient::new(recorded());
    let (mut model, _) = followed(&client, None);
    model.runner_buses.clear();
    let mut feeds = client.subscribe(Some(Seq(12)));
    while let Some(Feed::Event(event)) = block_on(feeds.next()) {
        drop(model.fold_event(event.kind));
    }
    assert!(
        model.runner_buses.is_empty(),
        "the bus change came before the resume point"
    );
    assert_eq!(model.environments.names(), [name(SIDE), name(MAIN)]);
}
