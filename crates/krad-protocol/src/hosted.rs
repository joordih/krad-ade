use krad_domain::{
    AgentState, Effort, EnvironmentData, EnvironmentName, HostedOrigin, Session, SessionId,
    SessionLook, Spend, TerminalId, WebhookTarget,
};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::chats::ChatState;
use crate::ids::Seq;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HostedChat {
    pub target: WebhookTarget,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<Effort>,
    #[serde(default)]
    pub asks: usize,
    #[serde(default)]
    pub spend: Spend,
    #[serde(default)]
    pub last_activity: Option<SystemTime>,
    #[serde(default)]
    pub seq: Option<Seq>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct HostedSummary {
    pub environment: EnvironmentName,
    pub session: SessionId,
    pub name: String,
    #[serde(default)]
    pub look: SessionLook,
    #[serde(default)]
    pub origin: Option<HostedOrigin>,
    pub branch: String,
    #[serde(default)]
    pub chats: Vec<HostedChat>,
    #[serde(default)]
    pub spend: Spend,
    #[serde(default)]
    pub last_activity: Option<SystemTime>,
    #[serde(default)]
    pub seq: Option<Seq>,
}

#[derive(Clone, Copy, Debug)]
pub struct ChatFacts<'a> {
    pub state: &'a ChatState,
    pub asks: usize,
    pub last_activity: Option<SystemTime>,
    pub seq: Option<Seq>,
}

impl HostedSummary {
    #[must_use]
    pub fn state(&self) -> AgentState {
        self.chats
            .iter()
            .map(|chat| chat.target.state)
            .max_by_key(|state| urgency(*state))
            .unwrap_or(AgentState::Idle)
    }
}

const fn urgency(state: AgentState) -> u8 {
    match state {
        AgentState::WaitingForInput => 3,
        AgentState::Working => 2,
        AgentState::Done => 1,
        AgentState::Idle => 0,
    }
}

fn chat_state(node_state: AgentState, facts: Option<&ChatFacts<'_>>) -> AgentState {
    match facts {
        Some(facts) if facts.asks > 0 => AgentState::WaitingForInput,
        Some(facts) if facts.state.working => AgentState::Working,
        _ => node_state,
    }
}

fn hosted_chat<'a>(
    data: &EnvironmentData,
    session: &Session,
    terminal: TerminalId,
    facts: &impl Fn(TerminalId) -> Option<ChatFacts<'a>>,
) -> Option<HostedChat> {
    let node = data.nodes.iter().find(|node| node.id == terminal)?;
    let process = node.cloud.as_ref()?.id.clone();
    let known = facts(terminal);
    Some(HostedChat {
        target: WebhookTarget {
            process,
            environment: session
                .cloud
                .as_ref()
                .and_then(|placement| placement.origin.as_ref())
                .map_or_else(|| data.name.clone(), |origin| origin.environment.clone()),
            group: session
                .cloud
                .as_ref()
                .and_then(|placement| placement.origin.as_ref())
                .map(|origin| origin.group),
            terminal,
            harness: known.map(|facts| facts.state.harness).or(node.harness),
            state: chat_state(node.state, known.as_ref()),
        },
        model: known.and_then(|facts| facts.state.model.clone()),
        effort: known.and_then(|facts| facts.state.effort),
        asks: known.map_or(0, |facts| facts.asks),
        spend: data.chat_spend(terminal),
        last_activity: known.and_then(|facts| facts.last_activity),
        seq: known.and_then(|facts| facts.seq),
    })
}

#[must_use]
pub fn hosted_summaries<'a>(
    data: &EnvironmentData,
    facts: impl Fn(TerminalId) -> Option<ChatFacts<'a>>,
) -> Vec<HostedSummary> {
    data.sessions
        .iter()
        .filter_map(|session| {
            let placement = session.cloud.as_ref()?;
            let chats: Vec<HostedChat> = session
                .tabs
                .tabs
                .iter()
                .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id))
                .filter_map(|terminal| hosted_chat(data, session, terminal, &facts))
                .collect();
            Some(HostedSummary {
                environment: data.name.clone(),
                session: session.id,
                name: session.name.clone(),
                look: session.look,
                origin: placement.origin.clone(),
                branch: placement.branch.clone(),
                spend: data.session_spend(session),
                last_activity: chats.iter().filter_map(|chat| chat.last_activity).max(),
                seq: chats.iter().filter_map(|chat| chat.seq).max(),
                chats,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
