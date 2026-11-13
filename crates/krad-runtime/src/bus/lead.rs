use krad_agents::{AgentInfo, Caller, ErrorCode, LeadHandle, Request, Response, TerminalState};
use krad_domain::{worker_session_name, EnvironmentData, SessionId, TerminalId};

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

#[must_use]
pub fn closed_worker_infos(data: &EnvironmentData) -> Vec<AgentInfo> {
    data.sessions
        .iter()
        .filter_map(|session| session.lead.as_ref())
        .flat_map(|lead| &lead.workers)
        .filter(|worker| worker.session.is_none())
        .map(|worker| AgentInfo {
            environment: Some(data.name.as_str().to_owned()),
            terminal: worker.terminal.as_u64(),
            title: worker_session_name(&worker.task),
            group: worker
                .reopen
                .as_ref()
                .and_then(|reopen| reopen.group)
                .and_then(|group| data.group(group))
                .map(|group| group.name.clone()),
            state: TerminalState::Closed,
            harness: worker.reopen.as_ref().map(|reopen| reopen.harness),
            is_self: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{
        AgentPreset, AgentState, EnvironmentName, ResumeId, SessionLead, WorkerSpawn,
    };
    use std::path::PathBuf;

    #[test]
    fn closed_workers_are_listed_only_as_closed() {
        let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
        let api = data.add_group(PathBuf::from("/work/api"), None);
        data.enter_sessions();
        let lead = data.new_session(Some(api), krad_domain::SessionSpec::default());
        if let Some(session) = data.sessions.iter_mut().find(|s| s.id == lead) {
            session.lead = Some(SessionLead::new(AgentPreset::ClaudeCode, None));
        }
        let worker = TerminalId(data.next_id);
        let node = krad_domain::TerminalNode {
            id: worker,
            title: "Claude Code · worker".to_owned(),
            state: AgentState::Done,
            harness: Some(AgentPreset::ClaudeCode),
            resume: "8d0d5ac1-7a3e-4f26-9b1c-2f4b3f1c9a10"
                .parse::<ResumeId>()
                .ok(),
            group: Some(api),
            text_scale: krad_domain::TextScale::default(),
            kind: krad_domain::NodeKind::default(),
            file: None,
            extension: None,
            bookmarks: krad_domain::ChatBookmarks::default(),
            usage: krad_domain::ChatUsage::default(),
            checkpoints: krad_domain::ChatCheckpoints::default(),
            choice: krad_domain::ChatChoice::default(),
            cloud: None,
        };
        data.nodes.push(node);
        let spawn = WorkerSpawn {
            lead: Some(lead),
            task: "Fix the webhook retries\nwith tests".to_owned(),
        };
        data.open_worker_session(worker, spawn);
        assert_eq!(closed_worker_infos(&data), Vec::<AgentInfo>::new());
        data.drop_terminal(worker);
        let listed = closed_worker_infos(&data);
        assert_eq!(
            listed,
            vec![AgentInfo {
                environment: Some(data.name.as_str().to_owned()),
                terminal: worker.as_u64(),
                title: "Fix the webhook retries".to_owned(),
                group: data.group(api).map(|group| group.name.clone()),
                state: TerminalState::Closed,
                harness: Some(AgentPreset::ClaudeCode),
                is_self: false,
            }]
        );
    }
}
