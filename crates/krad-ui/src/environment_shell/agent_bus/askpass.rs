use krad_agents::{ErrorCode, Response};

const GIT_ONLY: &str = "Only git asks for a sign-in through the bus";

pub(super) fn git_only() -> Response {
    Response::error(ErrorCode::Forbidden, GIT_ONLY)
}
