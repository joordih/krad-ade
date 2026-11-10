use super::exit::Exit;
use super::render::render_agent;
use super::{emit, Output};
use crate::client::watch;
use crate::env::BusEnv;
use crate::handle::Caller;
use crate::protocol::{
    AgentChangeKind, AgentEvent, AgentSnapshot, AgentSubscription, Response, TerminalState,
};
use std::ops::ControlFlow;

const RESYNC_MARKER: &str = "  (resync: events were missed)";
const NO_REASON: &str = "";
const COLUMN: &str = "  ";

pub(super) fn run(bus: &BusEnv, from: &Caller, subscription: AgentSubscription, json: bool) -> i32 {
    let mut exit = Exit::Ok;
    let watched = watch(&bus.endpoint, &bus.token, from, subscription, |response| {
        if let Response::Error(error) = &response {
            exit = error.code.into();
            emit(Output::Stderr(error.message.clone()));
            return ControlFlow::Break(());
        }
        if let Some(line) = watch_line(&response, json) {
            emit(Output::Stdout(line));
        }
        ControlFlow::Continue(())
    });
    match watched {
        Ok(()) => exit.code(),
        Err(error) => {
            emit(Output::Stderr(error.to_string()));
            Exit::Bus.code()
        }
    }
}

pub(super) fn watch_line(response: &Response, json: bool) -> Option<String> {
    match response {
        Response::Heartbeat(_) => None,
        _ if json => serde_json::to_string(response).ok(),
        Response::AgentSnapshot(snapshot) => Some(render_snapshot(snapshot)),
        Response::AgentEvent(event) => Some(render_event(event)),
        _ => None,
    }
}

fn render_snapshot(snapshot: &AgentSnapshot) -> String {
    let resync = if snapshot.resync { RESYNC_MARKER } else { "" };
    let head = format!(
        "{} agents at seq {} (stream {}){resync}",
        snapshot.agents.len(),
        snapshot.seq,
        snapshot.stream
    );
    std::iter::once(head)
        .chain(snapshot.agents.iter().map(|agent| {
            let environment = agent.environment.as_deref().unwrap_or_default();
            format!("{environment}:{}", render_agent(agent))
        }))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_event(event: &AgentEvent) -> String {
    let change = match (event.kind, event.previous) {
        (AgentChangeKind::Opened, _) | (_, None) => format!("opened as {}", event.state),
        (AgentChangeKind::Closed, Some(previous)) => format!("closed (was {previous})"),
        (AgentChangeKind::StateChanged, Some(previous)) => {
            format!("{previous} -> {}", event.state)
        }
    };
    let reason = event.reason.map_or(NO_REASON.to_owned(), |reason| {
        format!("  ({})", reason.name())
    });
    let group = event
        .group
        .as_ref()
        .map_or_default(|group| format!("{COLUMN}group: {group}"));
    let closed = matches!(event.state, TerminalState::Closed);
    let title = if closed { "" } else { event.title.as_str() };
    format!(
        "#{}{COLUMN}{}{COLUMN}{}:{}{COLUMN}{title}{COLUMN}{change}{reason}{group}",
        event.seq, event.at, event.environment, event.terminal
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentInfo, Heartbeat, StateReason, StreamId, Timestamp};
    use krad_domain::{AgentPreset, AgentState};
    use std::time::UNIX_EPOCH;

    fn event(
        kind: AgentChangeKind,
        previous: Option<TerminalState>,
        state: TerminalState,
    ) -> AgentEvent {
        AgentEvent {
            seq: 7,
            at: Timestamp::from(UNIX_EPOCH),
            kind,
            environment: "main".to_owned(),
            terminal: 4,
            title: "Codex · 4".to_owned(),
            group: Some("api".to_owned()),
            harness: Some(AgentPreset::Codex),
            previous,
            state,
            coarse_state: state.into(),
            reason: StateReason::inferred(previous, state),
        }
    }

    #[test]
    fn events_print_one_line_each_and_heartbeats_stay_silent() {
        let working = TerminalState::Running(AgentState::Working);
        let done = TerminalState::Running(AgentState::Done);
        let changed = event(AgentChangeKind::StateChanged, Some(working), done);
        assert_eq!(
            watch_line(&Response::AgentEvent(Box::new(changed.clone())), false).as_deref(),
            Some("#7  1970-01-01T00:00:00.000Z  main:4  Codex · 4  working -> done  (turn_finished)  group: api")
        );
        let opened = event(AgentChangeKind::Opened, None, working);
        assert_eq!(
            watch_line(&Response::AgentEvent(Box::new(opened)), false).as_deref(),
            Some("#7  1970-01-01T00:00:00.000Z  main:4  Codex · 4  opened as working  group: api")
        );
        let closed = event(AgentChangeKind::Closed, Some(done), TerminalState::Closed);
        let closed_line =
            watch_line(&Response::AgentEvent(Box::new(closed)), false).unwrap_or_default();
        assert!(
            closed_line.ends_with("closed (was done)  group: api"),
            "{closed_line}"
        );
        let heartbeat = Response::Heartbeat(Heartbeat {
            stream: StreamId("s".to_owned()),
            seq: 7,
            at: Timestamp::from(UNIX_EPOCH),
        });
        assert_eq!(watch_line(&heartbeat, false), None);
        assert_eq!(watch_line(&heartbeat, true), None);
        let json = watch_line(&Response::AgentEvent(Box::new(changed)), true).unwrap_or_default();
        assert!(json.starts_with("{\"AgentEvent\":{\"seq\":7,"), "{json}");
    }

    #[test]
    fn a_snapshot_lists_every_agent_under_a_header() {
        let snapshot = AgentSnapshot {
            stream: StreamId("ab12".to_owned()),
            seq: 3,
            at: Timestamp::from(UNIX_EPOCH),
            resync: true,
            agents: vec![AgentInfo {
                environment: Some("main".to_owned()),
                terminal: 4,
                title: "Codex · 4".to_owned(),
                group: None,
                state: TerminalState::Running(AgentState::Idle),
                harness: Some(AgentPreset::Codex),
                is_self: false,
            }],
        };
        assert_eq!(
            watch_line(&Response::AgentSnapshot(Box::new(snapshot)), false).as_deref(),
            Some("1 agents at seq 3 (stream ab12)  (resync: events were missed)\nmain:4  Codex · 4  [idle]  agent: codex")
        );
    }
}
