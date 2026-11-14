use krad_client::RuntimeClient;
use krad_domain::{
    AgentPreset, ChatMessage, CloudRunnerId, EnvironmentName, Prompt, RuntimeId, Speaker,
    TerminalId,
};
use krad_protocol::{
    AgentRef, ChatCommand, ChatState, Command, ErrorCode, Event, EventKind, Feed, Problem,
    ProblemScope, Seq,
};
use std::sync::Arc;

use super::super::tests::{ready, runner_moved, runner_snapshot, RUNNER, RUNNER_ENVIRONMENT};
use super::super::{Heard, ViewModel};
use super::Reached;
use crate::fake_client::RecordedClient;

const BUILD: &str = "0.9.0";
const SLUG: &str = "api-1a2b";
const CLONE_FAILED: &str = "git clone failed";

fn chat() -> AgentRef {
    AgentRef {
        environment: EnvironmentName::new(RUNNER_ENVIRONMENT.to_owned()),
        terminal: TerminalId(4),
    }
}

fn event(seq: u64, kind: EventKind) -> Feed {
    Feed::Event(Event {
        seq: Seq(seq),
        kind,
    })
}

fn connected(model: &mut ViewModel, client: Arc<RecordedClient>) -> u64 {
    let runner = CloudRunnerId(RUNNER);
    runner_moved(model, ready());
    let connection = model.remotes.reaching(runner);
    let reached = Reached::Connected {
        client,
        build: BUILD.to_owned(),
        stop: None,
    };
    assert!(model.adopt_reached(runner, connection, reached).is_some());
    connection
}

#[test]
fn a_hosted_session_s_commands_go_to_its_runner_and_local_ones_stay_here() {
    let mut model = ViewModel::default();
    let remote = RecordedClient::new(Vec::new());
    connected(&mut model, Arc::clone(&remote));
    let local_client = RecordedClient::new(Vec::new());
    let local: Arc<dyn RuntimeClient> = local_client.clone();
    let send = Command::Chat(ChatCommand::Send {
        chat: chat(),
        prompt: Prompt::from("go"),
    });
    let runner = RuntimeId::Runner(CloudRunnerId(RUNNER));
    let routed = model
        .remotes
        .route(&local, runner)
        .map(|client| client.post(send.clone()));
    assert!(routed.is_some());
    let interrupt = Command::Chat(ChatCommand::Interrupt { chat: chat() });
    if let Some(client) = model.remotes.route(&local, RuntimeId::Local) {
        client.post(interrupt.clone());
    }
    assert_eq!(remote.sent(), [send]);
    assert_eq!(local_client.sent(), [interrupt]);
    let unknown = RuntimeId::Runner(CloudRunnerId(RUNNER + 1));
    assert!(model.remotes.route(&local, unknown).is_none());
    assert!(model.remotes.connected(CloudRunnerId(RUNNER)));
}

#[test]
fn a_runner_s_chat_round_trip_shows_in_its_remote_mirror() {
    let runner = CloudRunnerId(RUNNER);
    let mut model = ViewModel::default();
    let connection = connected(&mut model, RecordedClient::new(Vec::new()));
    let feeds = [
        Feed::Snapshot(runner_snapshot()),
        event(
            11,
            EventKind::ChatOpened {
                chat: chat(),
                state: Box::new(ChatState::new(AgentPreset::ClaudeCode)),
            },
        ),
        event(
            12,
            EventKind::ChatMessageAdded {
                chat: chat(),
                index: 0,
                message: Box::new(ChatMessage::new(Speaker::User, "go".to_owned())),
            },
        ),
        event(
            13,
            EventKind::ChatMessageAdded {
                chat: chat(),
                index: 1,
                message: Box::new(ChatMessage::new(Speaker::Assistant, "done".to_owned())),
            },
        ),
    ];
    for feed in feeds {
        assert_eq!(model.fold_remote(runner, connection, feed), Some(true));
    }
    let shown = model.remotes.chat(runner, &chat()).map(|chat| {
        chat.mirror
            .messages
            .iter()
            .map(|message| message.text.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(shown, Some(vec!["go".to_owned(), "done".to_owned()]));
}

#[test]
fn a_clone_failure_on_the_runner_is_heard_only_for_the_session_this_window_handed_over() {
    let runner = CloudRunnerId(RUNNER);
    let mut model = ViewModel::default();
    let connection = connected(&mut model, RecordedClient::new(Vec::new()));
    let failed = |slug: &str| {
        event(
            20,
            EventKind::Problem(Problem {
                scope: ProblemScope::CloudSession {
                    slug: slug.to_owned(),
                },
                code: ErrorCode::Unavailable,
                message: CLONE_FAILED.to_owned(),
            }),
        )
    };
    drop(model.take_news());
    model.fold_remote(runner, connection, failed("someone-else"));
    assert!(model.take_news().heard.is_empty());
    model.remotes.await_hosted(runner, SLUG.to_owned());
    model.fold_remote(runner, connection, failed(SLUG));
    let heard = model.take_news().heard;
    assert!(matches!(
        heard.as_slice(),
        [Heard::HostedFailed { failure, .. }] if failure.slug == SLUG && failure.message == CLONE_FAILED
    ));
    model.fold_remote(runner, connection, failed(SLUG));
    assert!(model.take_news().heard.is_empty(), "a failure is told once");
}

#[test]
fn reaching_a_lost_runner_again_keeps_what_it_showed() {
    let runner = CloudRunnerId(RUNNER);
    let mut model = ViewModel::default();
    let connection = connected(&mut model, RecordedClient::new(Vec::new()));
    model.fold_remote(runner, connection, Feed::Snapshot(runner_snapshot()));
    let again = model.remotes.reaching_again(runner, connection);
    assert!(again.is_some_and(|next| next != connection));
    assert_eq!(model.remotes.sessions().count(), 1);
    assert_eq!(
        model.fold_remote(runner, connection, Feed::Snapshot(runner_snapshot())),
        None
    );
}

#[test]
fn a_runner_s_chat_events_reach_the_shell_and_its_answers_reach_the_asker() {
    let runner = CloudRunnerId(RUNNER);
    let mut model = ViewModel::default();
    let remote = RecordedClient::new(Vec::new());
    let connection = connected(&mut model, Arc::clone(&remote));
    model.fold_remote(runner, connection, Feed::Snapshot(runner_snapshot()));
    let opened = EventKind::ChatOpened {
        chat: chat(),
        state: Box::new(ChatState::new(AgentPreset::ClaudeCode)),
    };
    model.fold_remote(runner, connection, event(11, opened.clone()));
    let heard = model.take_news().heard;
    assert!(heard
        .iter()
        .any(|heard| matches!(heard, Heard::RemoteReplaced(replaced) if *replaced == runner)));
    assert!(heard.iter().any(|heard| matches!(
        heard,
        Heard::RemoteChat { runner: from, kind } if *from == runner && **kind == opened
    )));
    let Some((client, answer, request)) = model.remotes.wait(runner) else {
        unreachable!("a connected runner takes a waiter");
    };
    assert!(Arc::ptr_eq(
        &client,
        &(Arc::clone(&remote) as Arc<dyn RuntimeClient>)
    ));
    let bytes = krad_protocol::Answer::ImageBytes {
        format: krad_domain::ImageFormat::Png,
        bytes: b"png".to_vec(),
    };
    let answered = EventKind::Answered {
        request,
        answer: Box::new(bytes.clone()),
    };
    model.fold_remote(runner, connection, event(12, answered));
    assert_eq!(futures::executor::block_on(answer).ok(), Some(bytes));
}
