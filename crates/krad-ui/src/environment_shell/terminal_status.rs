use krad_domain::{AgentState, ProcessRun};

use super::live_terminal::LiveTerminal;

use crate::components::StatusKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CardStatus {
    Agent(AgentState),
    Exited,
    Failed,
}

impl CardStatus {
    pub(super) const fn agent_state(self) -> Option<AgentState> {
        match self {
            Self::Agent(state) => Some(state),
            Self::Exited | Self::Failed => None,
        }
    }

    pub(super) fn of(state: AgentState, session: Option<&LiveTerminal>) -> Self {
        match session.map(|session| session.view.status().run) {
            Some(ProcessRun::Exited { .. }) => Self::Exited,
            Some(ProcessRun::Failed { .. }) => Self::Failed,
            Some(ProcessRun::Running) | None => Self::Agent(state),
        }
    }

    pub(super) const fn kind(self) -> StatusKind {
        match self {
            Self::Agent(state) => StatusKind::Agent(state),
            Self::Exited => StatusKind::Agent(AgentState::Idle),
            Self::Failed => StatusKind::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exited_shells_rest_like_idle_ones() {
        assert_eq!(
            CardStatus::Exited.kind(),
            StatusKind::Agent(AgentState::Idle)
        );
        assert_eq!(CardStatus::Failed.kind(), StatusKind::Failed);
    }
}
