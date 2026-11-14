use super::*;
use krad_domain::{AcpAgentId, EnvironmentName, SessionLead, TerminalNode};
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SHELL: TerminalId = TerminalId(1);
const CHAT: TerminalId = TerminalId(2);
const LOOSE: TerminalId = TerminalId(3);
const TASK: &str = "Fix the webhook retries";

fn node(id: TerminalId, kind: &str, group: Option<GroupId>) -> serde_json::Result<TerminalNode> {
    serde_json::from_value(serde_json::json!({
        "id": id.0, "title": "agent", "state": "Idle", "kind": kind,
        "group": group.map(|group| group.0)
    }))
}

fn grouped() -> Result<(EnvironmentData, SessionId), Box<dyn std::error::Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    let group = data.add_group(PathBuf::from("/work/api"), None);
    data.nodes = vec![
        node(SHELL, "Terminal", Some(group))?,
        node(CHAT, "Chat", Some(group))?,
        node(LOOSE, "Terminal", None)?,
    ];
    data.enter_sessions();
    let session = data.active_session.ok_or("no session")?;
    Ok((data, session))
}

fn order(asker: WorkerAsker, worktree: Option<&str>) -> WorkerOrder<'_> {
    WorkerOrder {
        asker,
        preset: AgentPreset::ClaudeCode,
        worktree,
        task: TASK,
    }
}

fn planned(planned: Result<SpawnPlan, Response>) -> Result<SpawnPlan, String> {
    planned.map_err(|refused| format!("{refused:?}"))
}

fn refused_code(planned: Result<SpawnPlan, Response>) -> Option<ErrorCode> {
    match planned {
        Err(Response::Error(error)) => Some(error.code),
        _ => None,
    }
}

#[test]
fn a_terminal_s_worker_gets_the_terminal_brief_and_a_chat_s_the_chat_brief() -> TestResult {
    let (data, _) = grouped()?;
    let acp = AcpSettings::default();
    let from_shell = planned(plan_worker(
        &data,
        &acp,
        &order(WorkerAsker::Caller(SHELL), None),
    ))?;
    assert_eq!(
        from_shell.brief,
        worker_brief(SHELL.0, cli_name(false), TASK)
    );
    assert_eq!(from_shell.spawn.lead, None);
    assert_eq!(from_shell.mode_of, Some(SHELL));
    let from_chat = planned(plan_worker(
        &data,
        &acp,
        &order(WorkerAsker::Caller(CHAT), None),
    ))?;
    assert_eq!(from_chat.brief, chat_worker_brief(TASK));
    assert_eq!(from_chat.group, from_shell.group);
    Ok(())
}

#[test]
fn a_lead_s_worker_is_owned_by_its_session_and_a_former_lead_is_refused() -> TestResult {
    let (mut data, session) = grouped()?;
    let acp = AcpSettings::default();
    assert_eq!(
        refused_code(plan_worker(
            &data,
            &acp,
            &order(WorkerAsker::Lead(session), None)
        )),
        Some(ErrorCode::Forbidden)
    );
    if let Some(led) = data.sessions.iter_mut().find(|led| led.id == session) {
        led.lead = Some(SessionLead {
            terminal: Some(CHAT),
            ..SessionLead::new(AgentPreset::ClaudeCode, None)
        });
    }
    let lead = planned(plan_worker(
        &data,
        &acp,
        &order(WorkerAsker::Lead(session), None),
    ))?;
    assert_eq!(lead.spawn.lead, Some(session));
    assert_eq!(lead.spawn.task, TASK);
    assert_eq!(lead.mode_of, Some(CHAT));
    Ok(())
}

#[test]
fn spawns_the_runtime_cannot_carry_are_answered_with_their_reason() -> TestResult {
    let (data, _) = grouped()?;
    let acp = AcpSettings::default();
    assert_eq!(
        refused_code(plan_worker(
            &data,
            &acp,
            &order(WorkerAsker::Caller(LOOSE), None)
        )),
        Some(ErrorCode::Unsupported)
    );
    let shell = WorkerOrder {
        preset: AgentPreset::Shell,
        ..order(WorkerAsker::Caller(SHELL), None)
    };
    assert_eq!(
        refused_code(plan_worker(&data, &acp, &shell)),
        Some(ErrorCode::Unsupported)
    );
    let unknown = WorkerOrder {
        preset: AgentPreset::Acp("nobody".parse::<AcpAgentId>()?),
        ..order(WorkerAsker::Caller(SHELL), None)
    };
    assert_eq!(
        refused_code(plan_worker(&data, &acp, &unknown)),
        Some(ErrorCode::NotFound)
    );
    Ok(())
}
