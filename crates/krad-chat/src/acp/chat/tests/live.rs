use krad_domain::{AcpAgentStart, ChatEvent, Prompt};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

use super::super::{spawn_acp, AcpConfig, AcpWanted};
use crate::acp::client_side::OpenBuffers;
use crate::control::{AcpEvent, LiveEvent};
use crate::live::{AcpOrder, BusVars, LiveHarness, LiveSession, Order};
use crate::process::Runner;

const AGENT_VAR: &str = "KRAD_ACP_LIVE_AGENT";
const DEFAULT_AGENT: &str = "opencode";
const AGENT_ID: &str = "opencode";
const TURN_WAIT: Duration = Duration::from_mins(3);

struct NoBuffers;

impl OpenBuffers for NoBuffers {
    fn unsaved_text(&self, _: &Path) -> Option<String> {
        None
    }

    fn written(&self, _: &Path, _: &str) {}
}

fn opencode() -> AcpAgentStart {
    let agent = krad_domain::built_in_acp_agents()
        .into_iter()
        .find(|agent| agent.id.as_str() == AGENT_ID)
        .unwrap_or_else(|| unreachable!("opencode is a built-in ACP agent"));
    AcpAgentStart {
        agent,
        secrets: HashMap::new(),
    }
}

fn next_until(
    events: &Receiver<LiveEvent>,
    mut wanted: impl FnMut(&LiveEvent) -> bool,
) -> Vec<LiveEvent> {
    let mut seen = Vec::new();
    loop {
        let event = events
            .recv_timeout(TURN_WAIT)
            .unwrap_or_else(|_| unreachable!("opencode went quiet after {seen:?}"));
        let done = wanted(&event);
        seen.push(event);
        if done {
            return seen;
        }
    }
}

#[test]
#[ignore = "runs the real opencode acp agent and a model turn"]
fn opencode_acp_opens_a_session_switches_mode_and_answers_a_prompt() {
    let program = std::env::var(AGENT_VAR).unwrap_or_else(|_| DEFAULT_AGENT.to_owned());
    let cwd = std::env::temp_dir().join(format!("krad-acp-live-{}", std::process::id()));
    std::fs::create_dir_all(&cwd)
        .unwrap_or_else(|error| unreachable!("no scratch folder: {error}"));
    let (sender, events) = channel();
    let chat = spawn_acp(
        AcpConfig {
            runner: Runner::Native(PathBuf::from(program)),
            agent: opencode(),
            cwd,
            mcp: Vec::new(),
            bus_vars: BusVars::default(),
            buffers: Arc::new(NoBuffers),
            model: None,
            wanted: AcpWanted::default(),
        },
        &LiveSession::Start(crate::live::new_session_id()),
        move |event| drop(sender.send(event)),
    );
    let opened = next_until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    assert!(opened
        .iter()
        .any(|event| matches!(event, LiveEvent::Chat(ChatEvent::Session(_)))));
    let modes = opened.iter().find_map(|event| match event {
        LiveEvent::Acp(AcpEvent::Modes(modes)) => Some(modes.clone()),
        _ => None,
    });
    let modes = modes.unwrap_or_else(|| unreachable!("opencode offers modes: {opened:?}"));
    assert!(opened.iter().any(|event| matches!(event, LiveEvent::Acp(AcpEvent::Models(offered)) if !offered.available.is_empty())));
    let other = modes
        .available
        .iter()
        .find(|mode| mode.id != modes.current)
        .unwrap_or_else(|| unreachable!("opencode offers more than one mode"))
        .id
        .clone();
    chat.order(Order::Acp(AcpOrder::Mode(other.clone())));
    next_until(
        &events,
        |event| matches!(event, LiveEvent::Acp(AcpEvent::Modes(modes)) if modes.current == other),
    );
    chat.order(Order::Acp(AcpOrder::Mode(modes.current.clone())));
    next_until(
        &events,
        |event| matches!(event, LiveEvent::Acp(AcpEvent::Modes(now)) if now.current == modes.current),
    );
    chat.order(Order::Send(Prompt::from(
        "Reply with the single word pong and nothing else.",
    )));
    let turn = next_until(&events, |event| {
        matches!(
            event,
            LiveEvent::Chat(ChatEvent::Done | ChatEvent::Error(_))
        )
    });
    let answer: String = turn
        .iter()
        .filter_map(|event| match event {
            LiveEvent::Chat(ChatEvent::TextDelta(text)) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        matches!(turn.last(), Some(LiveEvent::Chat(ChatEvent::Done))),
        "{turn:?}"
    );
    assert!(answer.to_lowercase().contains("pong"), "{answer}");
    chat.stop();
}
