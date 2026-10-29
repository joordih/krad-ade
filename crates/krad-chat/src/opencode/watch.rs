use krad_domain::{AgentState, ResumeId};

use super::chat::opencode_session_id;
use super::events::ServerEvent;
use super::server::OpencodeServer;

const fn state_of(event: &ServerEvent) -> Option<AgentState> {
    match event {
        ServerEvent::Started(_) | ServerEvent::ToolCalled(_) => Some(AgentState::Working),
        ServerEvent::PermissionAsked(_) | ServerEvent::FormCreated(_) => {
            Some(AgentState::WaitingForInput)
        }
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
            if let Some(state) = state_of(&delivery.event) {
                sink(state);
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_turn_reads_as_working_waiting_and_done() {
        let states: Vec<Option<AgentState>> = [
            r#"{"type":"session.execution.started","data":{"sessionID":"ses_a"}}"#,
            r#"{"type":"permission.asked","data":{"id":"per_1","sessionID":"ses_a","action":"edit","resources":["a.rs"]}}"#,
            r#"{"type":"session.text.ended","data":{"sessionID":"ses_a","text":"ok"}}"#,
            r#"{"type":"session.execution.succeeded","data":{"sessionID":"ses_a"}}"#,
        ]
        .iter()
        .map(|line| state_of(&serde_json::from_str(line).unwrap_or_else(|_| unreachable!("the fixture is valid"))))
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
}
