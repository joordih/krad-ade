use super::worker::{lead_environment, main_environment, until, Folder};
use super::{answered, call_from, TestResult, ENVIRONMENT, LEAD};
use futures::executor::block_on;
use krad_agents::{Request, Response};
use krad_chat::new_session_id;
use krad_domain::{
    AcpAgent, AgentPreset, EnvironmentEdit, EnvironmentName, NodeKind, RuntimeState, SessionEdit,
    SessionLead, Settings, Speaker, TerminalId,
};
use krad_protocol::{AgentRef, Command, EventKind, Reply};
use krad_runtime::{BusSetup, EnvironmentsSetup, Runtime, RuntimeSetup, SettingsSetup};
use std::error::Error;
use std::path::Path;

const ACP_AGENT: &str = "tests/fixtures/fake-harness/acp-agent";
const ACP_ID: &str = "fake";
const ACP_ANSWER: &str = "pong in ask mode";
const CONFIG_HOME_VAR: &str = "XDG_CONFIG_HOME";
const TASK: &str = "Find why the rotation races";

fn acp_settings() -> Result<(Settings, AgentPreset), Box<dyn Error>> {
    let program = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(ACP_AGENT)
        .canonicalize()?;
    let agent = AcpAgent {
        id: ACP_ID.parse().map_err(|_| "the fake agent id is valid")?,
        name: "Fake".to_owned(),
        program: program.to_string_lossy().into_owned(),
        args: Vec::new(),
        env: Vec::new(),
        terminal: None,
    };
    let preset = AgentPreset::Acp(agent.id);
    let mut settings = Settings::default();
    settings.acp.add_agent(agent)?;
    Ok((settings, preset))
}

#[test]
fn a_lead_starts_an_acp_worker_the_runtime_launches_itself() -> TestResult {
    let folder =
        Folder(std::env::temp_dir().join(format!("krad-acp-lead-{}", new_session_id().as_str())));
    std::fs::create_dir_all(folder.0.join("config"))?;
    std::env::set_var(CONFIG_HOME_VAR, folder.0.join("config"));
    let data = lead_environment(&folder.0)?;
    let session = data
        .active_session
        .ok_or("sessions mode opened no session")?;
    let (settings, preset) = acp_settings()?;
    let runtime = Runtime::start(RuntimeSetup {
        bus: Some(BusSetup::default()),
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![data],
                ..RuntimeState::default()
            },
            file: None,
        },
        settings: SettingsSetup {
            settings,
            file: None,
        },
        ..RuntimeSetup::default()
    })?;
    let mut events = runtime.subscribe();
    let made_lead: Command = (
        EnvironmentName::new(ENVIRONMENT.to_owned()),
        EnvironmentEdit::from(SessionEdit::MakeLead {
            session,
            lead: SessionLead {
                terminal: Some(LEAD),
                ..SessionLead::new(AgentPreset::ClaudeCode, None)
            },
        }),
    )
        .into();
    assert_eq!(
        block_on(runtime.send(runtime.next_request(), made_lead))?,
        Reply::Accepted
    );
    let spawn = Request::Spawn {
        agent: preset,
        worktree: None,
        task: TASK.to_owned(),
    };
    let Response::Spawned { terminal } = answered(call_from(&runtime, LEAD, spawn)?)? else {
        return Err("the lead's ACP worker did not start".into());
    };
    let worker = AgentRef {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        terminal: TerminalId(terminal),
    };
    until(&mut events, |kind| {
        matches!(kind, EventKind::ChatMessageAdded { chat, message, .. }
            | EventKind::ChatMessageChanged { chat, message, .. }
            if *chat == worker && message.speaker == Speaker::Assistant
                && message.text == ACP_ANSWER)
    })?;
    let started = main_environment(&runtime)?;
    let node = started
        .nodes
        .iter()
        .find(|node| node.id == worker.terminal)
        .ok_or("the worker has no node")?;
    assert_eq!(node.kind, NodeKind::Chat);
    assert_eq!(node.harness, Some(preset));
    Ok(())
}
