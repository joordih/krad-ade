use super::chat_support::{chat, Ask as _, ChatFeed, TestResult, CLAUDE_HARNESS};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AgentPreset, EnvironmentChange, EnvironmentData, EnvironmentName, GroupId, McpProjectTrust,
    Prompt, RuntimeState, SessionLead, Settings, Speaker, TerminalGroup, TerminalId, TerminalNode,
    PROJECT_MCP_FILE,
};
use krad_protocol::{
    AgentRef, ChatCommand, ChatMirror, Command, EnvironmentCommand, ErrorCode, EventKind, Reply,
    SettingsCommand,
};
use krad_runtime::{
    BusSetup, ChatBrief, ChatStart, EnvironmentsSetup, Runtime, RuntimeHandle, RuntimeSetup,
    SettingsSetup,
};
use std::error::Error;
use std::path::{Path, PathBuf};

const ENVIRONMENT: &str = "main";
const PROMPT: &str = "Why does it race?";
const RECORDED_ANSWER: &str = "Two tabs rotate at once.";
const MISSING_PROGRAM: &str = "krad-test-no-such-claude";
const NOT_ON_PATH: &str = "is not on PATH";
const GATEWAY_UNREACHED: &str = "The Gateway is not reachable yet";
const ARGV_HARNESS: &str = "tests/fixtures/fake-harness/claude-argv";
const ARGV_FILE_VAR: &str = "KRAD_TEST_ARGV_FILE";
const MEMBER_HINT: &str = "You run inside Krad ADE";
const LEAD_HINT: &str = "You lead this Krad ADE session";
const PROJECT_SERVERS: &str =
    r#"{"mcpServers":{"files":{"command":"npx","args":["files"],"env":{"TOKEN":"kept-out"}}}}"#;

fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("krad-{name}-{}", new_session_id().as_str()));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn node_in(id: u64, group: GroupId) -> Result<TerminalNode, serde_json::Error> {
    serde_json::from_value(serde_json::json!({
        "id": id, "title": "chat", "state": "Idle", "group": group.0,
    }))
}

fn environment(root: &Path, nodes: &[u64]) -> Result<EnvironmentData, Box<dyn Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new(ENVIRONMENT.to_owned()));
    let group = GroupId(1);
    data.groups = vec![TerminalGroup {
        id: group,
        name: "work".to_owned(),
        root: root.to_path_buf(),
        worktree: None,
        activation: krad_domain::GroupActivation::default(),
    }];
    data.nodes = nodes
        .iter()
        .map(|id| node_in(*id, group))
        .collect::<Result<_, _>>()?;
    Ok(data)
}

fn start(
    environment: EnvironmentData,
    settings: Settings,
) -> Result<RuntimeHandle, Box<dyn Error>> {
    start_with_bus(environment, settings, None)
}

fn start_with_bus(
    environment: EnvironmentData,
    settings: Settings,
    bus: Option<BusSetup>,
) -> Result<RuntimeHandle, Box<dyn Error>> {
    Ok(Runtime::start(RuntimeSetup {
        bus,
        environments: EnvironmentsSetup {
            state: RuntimeState {
                environments: vec![environment],
                ..RuntimeState::default()
            },
            file: None,
        },
        settings: SettingsSetup {
            settings,
            file: None,
        },
        ..RuntimeSetup::default()
    })?)
}

fn claude_at(program: &str) -> Settings {
    let mut settings = Settings::default();
    settings
        .harness_commands
        .insert(AgentPreset::ClaudeCode, program.to_owned());
    settings
}

fn briefed(harness: AgentPreset) -> ChatStart {
    ChatStart {
        harness,
        brief: ChatBrief::default(),
        prepared: None,
        session: LiveSession::Start(new_session_id()),
        messages: Vec::new(),
        queued: Vec::new(),
        notices: Vec::new(),
    }
}

fn send(id: &AgentRef) -> Command {
    Command::Chat(ChatCommand::Send {
        chat: id.clone(),
        prompt: Prompt::from(PROMPT),
    })
}

fn answered(mirror: &ChatMirror) -> bool {
    !mirror.state.working
        && mirror
            .messages
            .iter()
            .any(|message| message.speaker == Speaker::Assistant && message.text == RECORDED_ANSWER)
}

fn problem_has(mirror: &ChatMirror, text: &str) -> bool {
    mirror
        .state
        .problem
        .as_deref()
        .is_some_and(|problem| problem.contains(text))
}

#[test]
fn a_chat_opened_from_a_brief_resolves_its_launch_from_the_settings_it_has_when_it_starts(
) -> TestResult {
    let root = scratch("briefed-chat")?;
    let fake = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(CLAUDE_HARNESS)
        .canonicalize()?;
    let runtime = start(environment(&root, &[1])?, claude_at(MISSING_PROGRAM))?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(1);
    runtime.open_chat(id.clone(), briefed(AgentPreset::ClaudeCode));
    assert_eq!(runtime.send_now(send(&id))?, Reply::Accepted);
    feed.until_chat(&id, |mirror| problem_has(mirror, NOT_ON_PATH))?;
    assert_eq!(feed.mirror(&id)?.state.queued.len(), 1);
    let save = SettingsCommand::Save {
        settings: Box::new(claude_at(&fake.to_string_lossy())),
    };
    assert_eq!(runtime.send_now(Command::Settings(save))?, Reply::Accepted);
    feed.until_chat(&id, answered)?;
    assert_eq!(feed.mirror(&id)?.messages[0].text, PROMPT);
    Ok(())
}

#[test]
fn a_project_mcp_file_is_asked_about_once_and_the_first_answer_is_kept_in_the_settings(
) -> TestResult {
    let root = scratch("mcp-trust")?;
    std::fs::write(root.join(PROJECT_MCP_FILE), PROJECT_SERVERS)?;
    let runtime = start(environment(&root, &[1, 2])?, Settings::default())?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let (first, second) = (chat(1), chat(2));
    for id in [&first, &second] {
        runtime.open_chat(id.clone(), briefed(AgentPreset::Gateway));
        assert_eq!(runtime.send_now(send(id))?, Reply::Accepted);
        feed.until_chat(id, |mirror| mirror.state.mcp_trust.is_some())?;
    }
    let asked = feed.mirror(&first)?.state.mcp_trust.ok_or("no trust ask")?;
    assert_eq!(asked.folder, root);
    assert_eq!(asked.servers[0].summary, "npx files");
    assert!(
        !feed.saw(|event| serde_json::to_string(event).is_ok_and(|json| json.contains("kept-out")))
    );
    let trust = |id: &AgentRef| {
        Command::Chat(ChatCommand::TrustMcp {
            chat: id.clone(),
            trust: McpProjectTrust::Ignored,
        })
    };
    assert_eq!(runtime.send_now(trust(&first))?, Reply::Accepted);
    assert!(matches!(
        runtime.send_now(trust(&second))?,
        Reply::Refused {
            code: ErrorCode::AlreadyAnswered,
            ..
        }
    ));
    for id in [&first, &second] {
        feed.until_chat(id, |mirror| {
            mirror.state.mcp_trust.is_none() && problem_has(mirror, GATEWAY_UNREACHED)
        })?;
    }
    assert!(feed.saw(|event| matches!(
        event,
        EventKind::SettingsChanged { settings }
            if settings.mcp.project_trust(&root) == Some(McpProjectTrust::Ignored)
    )));
    Ok(())
}

fn answers(mirror: &ChatMirror) -> usize {
    mirror
        .messages
        .iter()
        .filter(|message| message.speaker == Speaker::Assistant && message.text == RECORDED_ANSWER)
        .count()
}

#[test]
fn a_chat_made_lead_restarts_with_the_lead_hint_on_its_next_message() -> TestResult {
    let root = scratch("lead-restart")?;
    let argv = root.join("argv");
    std::env::set_var(ARGV_FILE_VAR, &argv);
    let fake = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(ARGV_HARNESS)
        .canonicalize()?;
    let mut data = environment(&root, &[1])?;
    data.enter_sessions();
    let runtime = start_with_bus(
        data.clone(),
        claude_at(&fake.to_string_lossy()),
        Some(BusSetup::default()),
    )?;
    let mut feed = ChatFeed::follow(&runtime)?;
    let id = chat(1);
    runtime.open_chat(id.clone(), briefed(AgentPreset::ClaudeCode));
    assert_eq!(runtime.send_now(send(&id))?, Reply::Accepted);
    feed.until_chat(&id, |mirror| !mirror.state.working && answers(mirror) == 1)?;
    let (index, mut session) = data
        .sessions
        .iter()
        .cloned()
        .enumerate()
        .next()
        .ok_or("sessions mode opened no session")?;
    session.lead = Some(SessionLead {
        terminal: Some(TerminalId(1)),
        ..SessionLead::new(AgentPreset::ClaudeCode, None)
    });
    let made_lead = EnvironmentCommand::Change {
        environment: EnvironmentName::new(ENVIRONMENT.to_owned()),
        changes: vec![EnvironmentChange::Session { index, session }],
    };
    assert_eq!(
        runtime.send_now(Command::Environments(made_lead))?,
        Reply::Accepted
    );
    assert_eq!(runtime.send_now(send(&id))?, Reply::Accepted);
    feed.until_chat(&id, |mirror| !mirror.state.working && answers(mirror) == 2)?;
    let started = std::fs::read_to_string(&argv)?;
    let runs: Vec<&str> = started.lines().collect();
    assert_eq!(runs.len(), 2);
    assert!(runs[0].contains(MEMBER_HINT) && !runs[0].contains(LEAD_HINT));
    assert!(runs[1].contains(LEAD_HINT));
    Ok(())
}
