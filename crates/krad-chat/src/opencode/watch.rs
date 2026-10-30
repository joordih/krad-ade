use krad_domain::{AgentState, ResumeId};

use super::chat::opencode_session_id;
use super::events::ServerEvent;
use super::server::OpencodeServer;

const fn state_of(event: &ServerEvent, nested: bool) -> Option<AgentState> {
    match event {
        ServerEvent::PermissionAsked(_) | ServerEvent::FormCreated(_) => {
            Some(AgentState::WaitingForInput)
        }
        _ if nested => None,
        ServerEvent::Started(_) | ServerEvent::ToolCalled(_) => Some(AgentState::Working),
        ServerEvent::Succeeded(_) | ServerEvent::Failed(_) | ServerEvent::Interrupted(_) => {
            Some(AgentState::Done)
        }
        _ => None,
    }
}

pub fn watch_opencode_session(
    server: &OpencodeServer,
    resume: &ResumeId,
    mut sink: impl FnMut(AgentState) + Send + 'static,
) {
    let session = opencode_session_id(resume);
    server.subscribe(
        &session,
        Box::new(move |delivery| {
            if let Some(state) = state_of(&delivery.event, delivery.nested) {
                sink(state);
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(line: &str) -> ServerEvent {
        serde_json::from_str(line).unwrap_or_else(|_| unreachable!("the fixture is valid"))
    }

    #[test]
    fn a_terminal_turn_reads_as_working_waiting_and_done() {
        let states: Vec<Option<AgentState>> = [
            r#"{"type":"session.execution.started","data":{"sessionID":"ses_a"}}"#,
            r#"{"type":"permission.asked","data":{"id":"per_1","sessionID":"ses_a","action":"edit","resources":["a.rs"]}}"#,
            r#"{"type":"session.text.ended","data":{"sessionID":"ses_a","text":"ok"}}"#,
            r#"{"type":"session.execution.succeeded","data":{"sessionID":"ses_a"}}"#,
        ]
        .iter()
        .map(|line| state_of(&event(line), false))
        .collect();
        assert_eq!(
            states,
            [
                Some(AgentState::Working),
                Some(AgentState::WaitingForInput),
                None,
                Some(AgentState::Done),
            ]
        );
    }

    #[test]
    fn a_subagent_finishing_leaves_its_parent_working_but_its_asks_still_wait() {
        let states: Vec<Option<AgentState>> = [
            r#"{"type":"session.execution.started","data":{"sessionID":"ses_child"}}"#,
            r#"{"type":"session.execution.succeeded","data":{"sessionID":"ses_child"}}"#,
            r#"{"type":"session.execution.interrupted","data":{"sessionID":"ses_child"}}"#,
            r#"{"type":"permission.asked","data":{"id":"per_1","sessionID":"ses_child","action":"edit","resources":["a.rs"]}}"#,
        ]
        .iter()
        .map(|line| state_of(&event(line), true))
        .collect();
        assert_eq!(
            states,
            [None, None, None, Some(AgentState::WaitingForInput)]
        );
    }
}
