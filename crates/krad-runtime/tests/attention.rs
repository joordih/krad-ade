use futures::executor::block_on;
use futures::StreamExt as _;
use krad_domain::{AgentState, AttentionKind, AttentionTarget, EnvironmentName, Sight, TerminalId};
use krad_protocol::{
    AttentionCommand, AttentionNotice, AttentionSnapshot, Command, EventKind, Feed, Reply, Scope,
};
use krad_runtime::{EventReceiver, Runtime, RuntimeHandle, RuntimeSetup};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const ENVIRONMENT: &str = "main";
const TITLE: &str = "claude";
const FEED_ENDED: &str = "the feed ended";
const NOT_A_SNAPSHOT: &str = "the feed did not start from a snapshot";

fn target(terminal: u64) -> AttentionTarget {
    AttentionTarget::new(
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        TerminalId(terminal),
    )
}

fn send(runtime: &RuntimeHandle, command: AttentionCommand) -> Result<Reply, Box<dyn Error>> {
    Ok(block_on(runtime.send(
        runtime.next_request(),
        Command::Attention(command),
    ))?)
}

fn raise(terminal: u64, kind: AttentionKind, sight: Sight) -> AttentionCommand {
    AttentionCommand::Raise {
        target: target(terminal),
        kind,
        sight,
        title: TITLE.to_owned(),
    }
}

fn subscribed(runtime: &RuntimeHandle) -> Result<EventReceiver, Box<dyn Error>> {
    let mut events = runtime.subscribe();
    match block_on(events.next()).ok_or(FEED_ENDED)? {
        Feed::Snapshot(_) => Ok(events),
        Feed::Event(_) => Err(NOT_A_SNAPSHOT.into()),
    }
}

fn next_event(events: &mut EventReceiver) -> Result<EventKind, Box<dyn Error>> {
    loop {
        match block_on(events.next()).ok_or(FEED_ENDED)? {
            Feed::Event(event) => return Ok(event.kind),
            Feed::Snapshot(_) => {}
        }
    }
}

fn raised_kind(event: &EventKind) -> Option<(AttentionTarget, AttentionKind)> {
    match event {
        EventKind::AttentionRaised(AttentionNotice { target, kind, .. }) => {
            Some((target.clone(), *kind))
        }
        _ => None,
    }
}

fn snapshot(runtime: &RuntimeHandle) -> Result<AttentionSnapshot, Box<dyn Error>> {
    Ok(block_on(runtime.snapshot(Scope::Runtime))?.attention)
}

#[test]
fn a_raise_is_announced_then_seen_leaves_a_finish_but_keeps_a_permission() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = subscribed(&runtime)?;
    send(
        &runtime,
        raise(1, AttentionKind::Finished, Sight::Unwatched),
    )?;
    send(
        &runtime,
        raise(2, AttentionKind::Permission, Sight::Unwatched),
    )?;
    assert_eq!(
        raised_kind(&next_event(&mut events)?),
        Some((target(1), AttentionKind::Finished))
    );
    let EventKind::AttentionRaised(notice) = next_event(&mut events)? else {
        return Err("no permission raised".into());
    };
    assert_eq!(notice.kind, AttentionKind::Permission);
    assert_eq!(notice.title, TITLE);
    for terminal in [1, 2] {
        send(
            &runtime,
            AttentionCommand::Seen {
                target: target(terminal),
            },
        )?;
    }
    assert_eq!(
        next_event(&mut events)?,
        EventKind::AttentionLeft { target: target(1) }
    );
    let kept = snapshot(&runtime)?;
    let waiting: Vec<(AttentionTarget, AttentionKind)> = kept
        .waiting
        .into_iter()
        .map(|entry| (entry.target, entry.kind))
        .collect();
    assert_eq!(waiting, vec![(target(2), AttentionKind::Permission)]);
    assert_eq!(kept.history.len(), 2);
    send(&runtime, AttentionCommand::Dismiss { target: target(2) })?;
    assert_eq!(
        next_event(&mut events)?,
        EventKind::AttentionLeft { target: target(2) }
    );
    send(&runtime, AttentionCommand::ClearHistory)?;
    assert_eq!(next_event(&mut events)?, EventKind::AttentionHistoryCleared);
    assert_eq!(snapshot(&runtime)?, AttentionSnapshot::default());
    Ok(())
}

#[test]
fn the_same_raise_is_announced_once_and_a_watched_finish_not_at_all() -> TestResult {
    let runtime = Runtime::start(RuntimeSetup::default())?;
    let mut events = subscribed(&runtime)?;
    send(&runtime, raise(1, AttentionKind::Ask, Sight::Unwatched))?;
    send(&runtime, raise(1, AttentionKind::Ask, Sight::Watched))?;
    send(&runtime, raise(2, AttentionKind::Finished, Sight::Watched))?;
    send(
        &runtime,
        AttentionCommand::Observe {
            target: target(3),
            previous: AgentState::Working,
            next: AgentState::WaitingForInput,
            waiting_for: AttentionKind::Permission,
            sight: Sight::Unwatched,
            title: TITLE.to_owned(),
        },
    )?;
    assert_eq!(
        raised_kind(&next_event(&mut events)?),
        Some((target(1), AttentionKind::Ask))
    );
    assert_eq!(
        raised_kind(&next_event(&mut events)?),
        Some((target(3), AttentionKind::Permission))
    );
    send(
        &runtime,
        AttentionCommand::Observe {
            target: target(3),
            previous: AgentState::WaitingForInput,
            next: AgentState::Working,
            waiting_for: AttentionKind::Permission,
            sight: Sight::Unwatched,
            title: TITLE.to_owned(),
        },
    )?;
    assert_eq!(
        next_event(&mut events)?,
        EventKind::AttentionLeft { target: target(3) }
    );
    assert_eq!(snapshot(&runtime)?.history.len(), 2);
    Ok(())
}
