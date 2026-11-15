use krad_chat::{new_session_id, LiveSession};
use krad_domain::{
    AgentPreset, AgentState, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatMessage, ChatUsage,
    EnvironmentChange, GroupId, NodeEdit, NodeKind, NodePlace, Speaker, TerminalId, TerminalNode,
    TextScale,
};
use krad_protocol::{AgentRef, CloudCommand, Command, EventKind, NodeCommand, Reply};
use krad_runtime::{ChatBrief, ChatRole, ChatStart};

use super::cloud::{
    cloned_with_remote, environment, environment_data, install_id, send, start, temp_folder,
    use_fake_runner, TestResult, RUNNER,
};
use super::mirror::ChatFeed;
use super::repo::committed_repo;
use super::runner::{install_host_cli, start_daemon};
use super::scenarios::{answers, sent, settled_with, PROMPT};

const EARLIER_QUESTION: &str = "Where does the login test race?";
const EARLIER_ANSWER: &str = "Between the two token refreshes.";
const CHAT_TITLE: &str = "Claude Code · login";
const NO_SESSION: &str = "the environment has no session";

fn chat_node(id: TerminalId, group: GroupId) -> TerminalNode {
    TerminalNode {
        id,
        title: CHAT_TITLE.to_owned(),
        state: AgentState::Idle,
        harness: Some(AgentPreset::ClaudeCode),
        resume: None,
        group: Some(group),
        text_scale: TextScale::default(),
        kind: NodeKind::Chat,
        file: None,
        extension: None,
        bookmarks: ChatBookmarks::default(),
        usage: ChatUsage::default(),
        checkpoints: ChatCheckpoints::default(),
        choice: ChatChoice::default(),
        cloud: None,
    }
}

fn earlier() -> Vec<ChatMessage> {
    vec![
        ChatMessage::new(Speaker::User, EARLIER_QUESTION.to_owned()),
        ChatMessage::new(Speaker::Assistant, EARLIER_ANSWER.to_owned()),
    ]
}

fn local_chat(messages: Vec<ChatMessage>) -> ChatStart {
    ChatStart {
        harness: AgentPreset::ClaudeCode,
        brief: ChatBrief {
            role: ChatRole::Agent,
            ..ChatBrief::default()
        },
        prepared: None,
        session: LiveSession::Start(new_session_id()),
        messages,
        queued: Vec::new(),
        notices: Vec::new(),
    }
}

fn placed_on_the_runner(event: &EventKind) -> bool {
    let EventKind::EnvironmentChanged { changes, .. } = event else {
        return false;
    };
    changes.iter().any(|change| {
        matches!(change, EnvironmentChange::Session { session, .. }
            if session.cloud.as_ref().is_some_and(|cloud| cloud.runner == RUNNER))
    })
}

pub fn a_session_sent_to_the_cloud_moves_in_place_and_its_chat_goes_on_on_the_runner() -> TestResult
{
    let config = temp_folder("mvc")?;
    let runner_home = temp_folder("mvh")?;
    use_fake_runner(&config.0, &runner_home.0);
    let origin = committed_repo("mv-origin")?;
    let local = cloned_with_remote(&origin, "mvl")?;
    install_host_cli(&runner_home.0)?;
    let _daemon = start_daemon(&runner_home.0)?;
    let started = start(&local.0.join("work"), install_id()?)?;
    let runtime = started.runtime.clone();
    let mut chats = ChatFeed::follow(&runtime);

    let before = environment_data(&runtime)?;
    let session = before.sessions.first().ok_or(NO_SESSION)?.clone();
    let id = AgentRef {
        environment: environment(),
        terminal: TerminalId(before.next_id),
    };
    let opened = NodeEdit::Open {
        node: Box::new(chat_node(id.terminal, started.group)),
        place: NodePlace::Tab { anchor: None },
    };
    let node = Command::Node(NodeCommand {
        environment: environment(),
        edit: opened,
    });
    assert_eq!(send(&runtime, node)?, Reply::Accepted);
    runtime.open_chat(id.clone(), local_chat(earlier()));
    chats.until(|event| matches!(event, EventKind::ChatOpened { chat, .. } if *chat == id))?;
    assert_eq!(
        environment_data(&runtime)?.session_of(id.terminal),
        Some(session.id)
    );

    let move_it = CloudCommand::Send {
        environment: environment(),
        session: session.id,
        runner: RUNNER,
        prompt: None,
    };
    assert_eq!(send(&runtime, Command::Cloud(move_it))?, Reply::Accepted);
    chats.until(placed_on_the_runner)?;

    let after = environment_data(&runtime)?;
    assert_eq!(after.sessions.len(), before.sessions.len());
    let moved = after
        .session(session.id)
        .ok_or("the session left its place")?;
    assert_eq!(moved.name, session.name);
    assert_eq!(
        after
            .sessions
            .iter()
            .position(|found| found.id == session.id),
        before
            .sessions
            .iter()
            .position(|found| found.id == session.id)
    );
    assert_eq!(after.session_of(id.terminal), Some(session.id));
    assert_eq!(after.active_session, before.active_session);
    let placement = moved
        .cloud
        .as_ref()
        .ok_or("the session is not in the cloud")?;
    assert!(placement.worktree.starts_with(&runner_home.0));

    chats.until_chat(&id, |mirror| settled_with(mirror, 1))?;
    assert_eq!(send(&runtime, sent(&id, PROMPT))?, Reply::Accepted);
    chats.until_chat(&id, |mirror| settled_with(mirror, 2))?;
    let mirror = chats.mirror(&id)?;
    let shown: Vec<&str> = mirror
        .messages
        .iter()
        .take(2)
        .map(|message| message.text.as_str())
        .collect();
    assert_eq!(shown, [EARLIER_QUESTION, EARLIER_ANSWER]);
    assert_eq!(answers(&mirror), 2);
    Ok(())
}
