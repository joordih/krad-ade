use krad_agents::{ErrorCode, Response};
use krad_runtime::BusReply;

use super::super::EnvironmentShell;

const ANSWERED_BY_RUNTIME: &str = "Kraddie and memory requests are answered by the runtime";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn answer_kraddie_request(reply: BusReply) {
        reply
            .send(Response::error(ErrorCode::Unsupported, ANSWERED_BY_RUNTIME))
            .unwrap_or_default();
    }
}
