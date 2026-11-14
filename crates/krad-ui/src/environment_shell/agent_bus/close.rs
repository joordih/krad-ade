use krad_domain::agents_api::{ApiError, ErrorCode};
use krad_domain::{CloseSubject, EnvironmentData, GroupId, SessionId, WhenWorking};
use krad_runtime::refusal_error;

const UNKNOWN_VARIANTS_RUN: &str = "No variants run with that id in your environment";
const FOREIGN_VARIANTS_RUN: &str = "Only the variants runs of your session's group can be picked";

#[must_use]
pub(super) fn session_close_refusal(
    data: &EnvironmentData,
    lead: SessionId,
    session: SessionId,
) -> Option<ApiError> {
    data.plan_close(
        lead,
        CloseSubject::Session(session),
        WhenWorking::CancelTurn,
        |_| false,
    )
    .err()
    .map(refusal_error)
}

#[must_use]
pub(super) fn pick_refusal(run_group: Option<GroupId>, lead_group: GroupId) -> Option<ApiError> {
    match run_group {
        None => Some(ApiError::new(ErrorCode::NotFound, UNKNOWN_VARIANTS_RUN)),
        Some(group) if group != lead_group => {
            Some(ApiError::new(ErrorCode::NotOwned, FOREIGN_VARIANTS_RUN))
        }
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests;
