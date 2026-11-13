use krad_agents::{Caller, ErrorCode, LeadHandle, Request, Response, TerminalState};
use krad_domain::{EnvironmentData, SessionId, TerminalId};

const WORKER_RETIRED: &str = "That worker finished and was closed; send it a message to reopen it";

fn closed_worker(data: &EnvironmentData, target: u64) -> Option<SessionId> {
    let (owner, worker) = data.worker(TerminalId(target))?;
    worker.session.is_none().then_some(owner)
}

#[must_use]
pub(crate) fn closed_worker_answer(
    from: &Caller,
    request: &Request,
    data: Option<&EnvironmentData>,
) -> Option<Response> {
    let data = data?;
    match (from, request) {
        (Caller::Terminal(_) | Caller::Lead(_), Request::Wait { target, .. }) => {
            closed_worker(data, *target)?;
            Some(Response::Settled {
                target: *target,
                state: TerminalState::Closed,
            })
        }
        (Caller::Lead(lead), Request::Read { target, .. }) => {
            let owner = closed_worker(data, *target)?;
            (owner.as_u64() == lead.session)
                .then(|| Response::error(ErrorCode::NotFound, WORKER_RETIRED))
        }
        _ => None,
    }
}

#[must_use]
pub(crate) fn as_lead(from: Caller, request: &Request, data: Option<&EnvironmentData>) -> Caller {
    let (Caller::Terminal(terminal), Some(data)) = (&from, data) else {
        return from;
    };
    let Some(session) = data.session_led_by(TerminalId(terminal.terminal)) else {
        return from;
    };
    let follows_own_worker = request
        .followed_terminal()
        .and_then(|target| data.worker(TerminalId(target)))
        .is_some_and(|(owner, _)| owner == session);
    if !request.directs_workers() && !follows_own_worker {
        return from;
    }
    Caller::Lead(LeadHandle {
        environment: terminal.environment.clone(),
        session: session.as_u64(),
    })
}
