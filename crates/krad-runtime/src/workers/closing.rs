use krad_agents::{ApiError, ClosedTarget, ErrorCode};
use krad_domain::{CloseRefusal, CloseSubject};

const UNKNOWN_TARGET: &str = "No worker or session with that id in your environment";
const ALREADY_CLOSED: &str = "That worker is already closed; send it a message to reopen it";
const NOT_OWNED: &str = "Only the workers you started and the sessions of your group can be closed";
const ITSELF: &str = "A lead cannot close itself or the session it runs in";
const WORKING: &str = "It is still working; pass --force to stop its turn and close it";

#[must_use]
pub fn refusal_error(refusal: CloseRefusal) -> ApiError {
    let (code, message) = match refusal {
        CloseRefusal::Unknown => (ErrorCode::NotFound, UNKNOWN_TARGET),
        CloseRefusal::AlreadyClosed => (ErrorCode::NotFound, ALREADY_CLOSED),
        CloseRefusal::NotOwned => (ErrorCode::NotOwned, NOT_OWNED),
        CloseRefusal::Itself => (ErrorCode::Forbidden, ITSELF),
        CloseRefusal::Working => (ErrorCode::Busy, WORKING),
    };
    ApiError::new(code, message)
}

#[must_use]
pub const fn closed_target(subject: CloseSubject) -> ClosedTarget {
    match subject {
        CloseSubject::Worker(terminal) => ClosedTarget::Agent(terminal.as_u64()),
        CloseSubject::Session(session) => ClosedTarget::Session(session.as_u64()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_refusal_answers_with_its_own_stable_code() {
        let refusals = [
            (CloseRefusal::Unknown, ErrorCode::NotFound),
            (CloseRefusal::AlreadyClosed, ErrorCode::NotFound),
            (CloseRefusal::NotOwned, ErrorCode::NotOwned),
            (CloseRefusal::Itself, ErrorCode::Forbidden),
            (CloseRefusal::Working, ErrorCode::Busy),
        ];
        let mut messages = HashSet::new();
        for (refusal, code) in refusals {
            let error = refusal_error(refusal);
            assert_eq!(error.code, code, "{refusal:?}");
            assert!(messages.insert(error.message), "{refusal:?}");
        }
        assert!(refusal_error(CloseRefusal::Working)
            .message
            .contains("--force"));
    }
}
