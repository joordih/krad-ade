use tracing::debug;

use crate::acp::connection::{AgentRequest, Responder};
use crate::acp::permission::{RequestPermissionRequest, RequestPermissionResponse};
use crate::acp::rpc::{ErrorCode, RpcError};

const NOT_SERVED_TEXT: &str = "Krad ADE does not serve this method yet";

pub(super) type PermissionAsked = (
    RequestPermissionRequest,
    Responder<RequestPermissionResponse>,
);

pub(super) fn serve_client_side(request: AgentRequest) -> Option<PermissionAsked> {
    let refusal = RpcError::new(ErrorCode::METHOD_NOT_FOUND, NOT_SERVED_TEXT);
    match request {
        AgentRequest::RequestPermission(asked, responder) => return Some((asked, responder)),
        AgentRequest::ReadTextFile(_, responder) => responder.fail(&refusal),
        AgentRequest::WriteTextFile(_, responder)
        | AgentRequest::KillTerminal(_, responder)
        | AgentRequest::ReleaseTerminal(_, responder) => responder.fail(&refusal),
        AgentRequest::CreateTerminal(_, responder) => responder.fail(&refusal),
        AgentRequest::TerminalOutput(_, responder) => responder.fail(&refusal),
        AgentRequest::WaitForTerminalExit(_, responder) => responder.fail(&refusal),
    }
    debug!("refused an ACP client-side request the app does not serve yet");
    None
}
