use super::worker::{led_runtime, main_environment, until, Folder};
use super::{TestResult, ENVIRONMENT, LEAD};
use futures::executor::block_on;
use futures::StreamExt as _;
use krad_chat::new_session_id;
use krad_domain::{AgentPreset, EnvironmentName, SessionId, TerminalId, WhenWorking, WorktreeFate};
use krad_protocol::{AgentAct, AgentCommand, AgentRef, Command, ErrorCode, EventKind, Feed, Reply};
use krad_runtime::RuntimeHandle;
use std::error::Error;

const TASK: &str = "Find why the rotation races";

fn agent_command(
    runtime: &RuntimeHandle,
    lead: SessionId,
    act: AgentAct,
) -> Result<Reply, Box<dyn Error>> {
    let command = Command::Agent(AgentCommand {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        lead,
        act,
    });
    Ok(block_on(runtime.send(runtime.next_request(), command))?)
}

fn close(worker: TerminalId) -> AgentAct {
    AgentAct::CloseWorker {
        worker,
        when_working: WhenWorking::CancelTurn,
        worktree: WorktreeFate::Keep,
    }
}

#[test]
fn a_client_starts_and_closes_a_lead_s_worker_with_commands() -> TestResult {
    let folder =
        Folder(std::env::temp_dir().join(format!("krad-lead-{}", new_session_id().as_str())));
    std::fs::create_dir_all(&folder.0)?;
    let runtime = led_runtime(&folder)?;
    let lead = main_environment(&runtime)?
        .session_of(LEAD)
        .ok_or("the lead has no session")?;
    let mut events = runtime.subscribe();
    let Some(Feed::Snapshot(_)) = block_on(events.next()) else {
        return Err("the feed did not start from a snapshot".into());
    };
    let started = agent_command(
        &runtime,
        lead,
        AgentAct::StartWorker {
            agent: AgentPreset::ClaudeCode,
            worktree: None,
            task: TASK.to_owned(),
        },
    )?;
    assert_eq!(started, Reply::Accepted);
    let worker = main_environment(&runtime)?
        .nodes
        .iter()
        .map(|node| node.id)
        .find(|id| *id != LEAD)
        .ok_or("no worker node opened")?;
    let chat = AgentRef {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        terminal: worker,
    };
    until(
        &mut events,
        |kind| matches!(kind, EventKind::ChatOpened { chat: heard, .. } if *heard == chat),
    )?;
    assert_eq!(
        agent_command(&runtime, lead, close(worker))?,
        Reply::Accepted
    );
    until(
        &mut events,
        |kind| matches!(kind, EventKind::ChatClosed { chat: heard } if *heard == chat),
    )?;
    assert!(main_environment(&runtime)?
        .nodes
        .iter()
        .all(|node| node.id != worker));
    let again = agent_command(&runtime, lead, close(worker))?;
    assert!(matches!(
        again,
        Reply::Refused {
            code: ErrorCode::NotFound,
            ..
        }
    ));
    Ok(())
}

#[test]
fn a_session_without_a_lead_cannot_start_a_worker() -> TestResult {
    let folder =
        Folder(std::env::temp_dir().join(format!("krad-lead-{}", new_session_id().as_str())));
    std::fs::create_dir_all(&folder.0)?;
    let runtime = led_runtime(&folder)?;
    let refused = agent_command(
        &runtime,
        SessionId(999),
        AgentAct::StartWorker {
            agent: AgentPreset::ClaudeCode,
            worktree: None,
            task: TASK.to_owned(),
        },
    )?;
    assert!(matches!(
        refused,
        Reply::Refused {
            code: ErrorCode::Forbidden,
            ..
        }
    ));
    Ok(())
}
