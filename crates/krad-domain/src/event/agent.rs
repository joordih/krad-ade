use crate::event::AppEvent;
use crate::ids::TerminalId;
use crate::{AgentPreset, AgentState};

#[must_use]
pub fn agent_event(
    terminal: TerminalId,
    harness: Option<AgentPreset>,
    before: AgentState,
    after: AgentState,
) -> Option<AppEvent> {
    if before == after || !harness.is_some_and(|harness| harness != AgentPreset::Shell) {
        return None;
    }
    match (before, after) {
        (_, AgentState::Working) => Some(AppEvent::AgentStarted { terminal, harness }),
        (_, AgentState::WaitingForInput) => Some(AppEvent::AgentWaiting { terminal }),
        (_, AgentState::Done) | (AgentState::Working, AgentState::Idle) => {
            Some(AppEvent::AgentFinished { terminal })
        }
        _ => None,
    }
}

#[must_use]
pub fn hook_state(harness: AgentPreset, event: &str, tool: Option<&str>) -> Option<AgentState> {
    harness.hook_signal(event, tool).map(AgentState::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERMINAL: TerminalId = TerminalId(5);

    #[test]
    fn agent_states_become_started_waiting_and_finished() {
        let claude = Some(AgentPreset::ClaudeCode);
        let event = |before, after| agent_event(TERMINAL, claude, before, after);
        assert_eq!(
            event(AgentState::Idle, AgentState::Working),
            Some(AppEvent::AgentStarted {
                terminal: TERMINAL,
                harness: claude,
            })
        );
        assert_eq!(
            event(AgentState::Working, AgentState::WaitingForInput),
            Some(AppEvent::AgentWaiting { terminal: TERMINAL })
        );
        let finished = Some(AppEvent::AgentFinished { terminal: TERMINAL });
        assert_eq!(event(AgentState::Working, AgentState::Done), finished);
        assert_eq!(event(AgentState::Working, AgentState::Idle), finished);
        assert_eq!(event(AgentState::Done, AgentState::Idle), None);
        assert_eq!(event(AgentState::Working, AgentState::Working), None);
        assert_eq!(
            event(AgentState::WaitingForInput, AgentState::WaitingForInput),
            None
        );
    }

    #[test]
    fn plain_shells_raise_no_agent_events() {
        let shell = |harness| agent_event(TERMINAL, harness, AgentState::Idle, AgentState::Working);
        assert_eq!(shell(None), None);
        assert_eq!(shell(Some(AgentPreset::Shell)), None);
    }

    #[test]
    fn a_hook_names_the_state_it_moves_the_agent_to() {
        assert_eq!(
            hook_state(AgentPreset::Codex, "agent-turn-complete", None),
            Some(AgentState::Done)
        );
        assert_eq!(hook_state(AgentPreset::Shell, "Stop", None), None);
    }
}
