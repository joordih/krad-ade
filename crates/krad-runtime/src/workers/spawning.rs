use futures::channel::oneshot;
use krad_agents::{ErrorCode, Response};
use krad_domain::{
    chat_worker_brief, parse_wsl_path, worker_brief, AcpSettings, AgentPreset, CloudPlacement,
    EnvironmentData, GroupId, NodeKind, PermissionMode, SessionId, TerminalId, WorkerSpawn,
};

use super::runs_as_worker;
use crate::launches::cli_name;

const NOT_A_LEAD_ANYMORE: &str = "This chat is no longer its session's lead";
const NO_LEAD_GROUP: &str = "The lead's session belongs to no group, so it cannot start workers";
const NO_GROUP: &str = "Run spawn from a terminal that belongs to a group";
const NOT_A_WORKER: &str = "That agent cannot work as a worker";
const UNKNOWN_ACP_AGENT: &str = "is not an ACP agent; add it in Settings, ACP agents";
const CLOUD_WORKTREE: &str =
    "Workers of a cloud session run in its worktree on the cloud runner; start them without --worktree";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkerAsker {
    Lead(SessionId),
    Caller(TerminalId),
}

#[derive(Debug)]
pub(crate) struct PendingWorker {
    pub(crate) spawn: WorkerSpawn,
    pub(crate) preset: AgentPreset,
    pub(crate) brief: String,
    pub(crate) mode: Option<PermissionMode>,
    pub(crate) cloud: Option<CloudPlacement>,
    pub(crate) reply: oneshot::Sender<Response>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SpawnPlan {
    pub(crate) group: GroupId,
    pub(crate) spawn: WorkerSpawn,
    pub(crate) brief: String,
    pub(crate) mode_of: Option<TerminalId>,
    pub(crate) cloud: Option<CloudPlacement>,
}

pub(crate) struct WorkerOrder<'a> {
    pub(crate) asker: WorkerAsker,
    pub(crate) preset: AgentPreset,
    pub(crate) worktree: Option<&'a str>,
    pub(crate) task: &'a str,
}

fn refused(code: ErrorCode, message: impl Into<String>) -> Response {
    Response::error(code, message)
}

fn lead_plan(data: &EnvironmentData, lead: SessionId, task: &str) -> Result<SpawnPlan, Response> {
    let session = data
        .session(lead)
        .filter(|session| session.lead.is_some())
        .ok_or_else(|| refused(ErrorCode::Forbidden, NOT_A_LEAD_ANYMORE))?;
    let group = session
        .group
        .filter(|group| data.group(*group).is_some())
        .ok_or_else(|| refused(ErrorCode::Forbidden, NO_LEAD_GROUP))?;
    Ok(SpawnPlan {
        group,
        spawn: WorkerSpawn {
            lead: Some(lead),
            task: task.to_owned(),
        },
        brief: chat_worker_brief(task),
        mode_of: session.lead.as_ref().and_then(|led| led.terminal),
        cloud: session.cloud.clone(),
    })
}

fn caller_plan(
    data: &EnvironmentData,
    caller: TerminalId,
    task: &str,
) -> Result<SpawnPlan, Response> {
    let group = data
        .group_of(caller)
        .ok_or_else(|| refused(ErrorCode::Unsupported, NO_GROUP))?;
    let from_chat = data
        .nodes
        .iter()
        .any(|node| node.id == caller && node.kind == NodeKind::Chat);
    let brief = if from_chat {
        chat_worker_brief(task)
    } else {
        let in_wsl = parse_wsl_path(&group.root).is_some();
        worker_brief(caller.as_u64(), cli_name(in_wsl), task)
    };
    let cloud = data
        .session_of(caller)
        .and_then(|session| data.session(session))
        .and_then(|session| session.cloud.clone());
    Ok(SpawnPlan {
        group: group.id,
        spawn: WorkerSpawn {
            lead: None,
            task: String::new(),
        },
        brief,
        mode_of: Some(caller),
        cloud,
    })
}

pub(crate) fn plan_worker(
    data: &EnvironmentData,
    acp: &AcpSettings,
    order: &WorkerOrder<'_>,
) -> Result<SpawnPlan, Response> {
    if let Some(agent) = order
        .preset
        .acp_agent()
        .filter(|agent| acp.agent(*agent).is_none())
    {
        return Err(refused(
            ErrorCode::NotFound,
            format!("{agent} {UNKNOWN_ACP_AGENT}"),
        ));
    }
    if !runs_as_worker(order.preset) {
        return Err(refused(ErrorCode::Unsupported, NOT_A_WORKER));
    }
    let plan = match order.asker {
        WorkerAsker::Lead(lead) => lead_plan(data, lead, order.task)?,
        WorkerAsker::Caller(caller) => caller_plan(data, caller, order.task)?,
    };
    if plan.cloud.is_some() && order.worktree.is_some() {
        return Err(refused(ErrorCode::Unsupported, CLOUD_WORKTREE));
    }
    Ok(plan)
}

#[cfg(test)]
mod tests;
