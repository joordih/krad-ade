use krad_domain::AgentState;
use krad_session::{LiveSession, SessionState};

use crate::components::StatusKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CardStatus {
    Agent(AgentState),
    Exited,
    Failed,
}

impl CardStatus {
    pub(super) fn of(state: AgentState, session: Option<&LiveSession>) -> Self {
        match session {
            Some(session) => match session.state {
                SessionState::Running => Self::Agent(state),
                SessionState::Exited(_) => Self::Exited,
                SessionState::Failed(_) => Self::Failed,
            },
            _ => Self::Agent(state),
        }
    }

    pub(super) const fn kind(self) -> StatusKind {
        match self {
            Self::Agent(state) => StatusKind::Agent(state),
            Self::Exited => StatusKind::Agent(AgentState::Idle),
            Self::Failed => StatusKind::Failed,
        }
    }

    pub(super) const fn is_quiet(self) -> bool {
        matches!(self, Self::Agent(AgentState::Idle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exited_shells_rest_like_idle_ones_but_still_say_so() {
        assert_eq!(
            CardStatus::Exited.kind(),
            StatusKind::Agent(AgentState::Idle)
        );
        assert!(!CardStatus::Exited.is_quiet());
        assert_eq!(CardStatus::Failed.kind(), StatusKind::Failed);
    }
}
