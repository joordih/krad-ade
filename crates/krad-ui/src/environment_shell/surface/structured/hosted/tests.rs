use super::super::super::place::route_chat;
use super::super::runtime_chat::rewind_command;
use super::*;
use crate::fake_client::RecordedClient;
use krad_client::RuntimeClient;
use krad_domain::{
    AgentPreset, AskAnswer, ChatMessage, EnvironmentData, EnvironmentName, ModelId, PickedAnswer,
    Prompt, RewindScope, Speaker,
};
use krad_protocol::{ChatAskId, ChatState, ErrorCode};
use std::sync::Arc;

use super::super::super::HostedPlaces;

fn runner_place() -> ChatPlace {
    ChatPlace::hosted(CloudRunnerId(1), EnvironmentName::new("runner".to_owned()))
}

#[test]
fn a_mirrored_chat_opens_driven_by_its_runner_with_the_runner_s_choices_and_tail() {
    let mut state = ChatState::new(AgentPreset::ClaudeCode);
    state.chosen_model = ModelId::parse("opus");
    state.effort = Some(krad_domain::Effort::High);
    let mirror = ChatMirror {
        state,
        from: 4,
        messages: vec![ChatMessage::new(Speaker::User, "hello".to_owned())],
        asks: Vec::new(),
    };
    let structured = hosted_structured(&mirror);
    assert!(structured.in_runtime());
    assert_eq!(structured.chosen_model, ModelId::parse("opus"));
    let place = runner_place();
    let summary = mirrored_summary(&place, TerminalId(7), mirror);
    assert_eq!(summary.tail.from, 4);
    assert_eq!(summary.tail.len, 5);
    assert_eq!(summary.tail.chat, place.chat(TerminalId(7)));
}

#[test]
fn a_hosted_chat_s_commands_reach_its_runner_and_a_local_chat_s_stay_here() {
    let place = runner_place();
    let id = TerminalId(3);
    let runner = RecordedClient::new(Vec::new());
    let local = RecordedClient::new(Vec::new());
    let local_client: Arc<dyn RuntimeClient> = Arc::clone(&local) as Arc<dyn RuntimeClient>;
    let mut hosted = HostedPlaces::default();
    let data = EnvironmentData::empty(place.environment.clone());
    hosted.follow(
        &place,
        Some(&data),
        Some(Arc::clone(&runner) as Arc<dyn RuntimeClient>),
    );
    let chat = place.chat(id);
    let commands = [
        ChatCommand::Send {
            chat: chat.clone(),
            prompt: Prompt::from("run the tests"),
        },
        ChatCommand::AnswerAsk {
            chat: chat.clone(),
            ask: ChatAskId(5),
            answer: AskAnswer::Answers(vec![PickedAnswer {
                question: "Which suite?".to_owned(),
                labels: vec!["Unit".to_owned()],
            }]),
        },
        ChatCommand::SetModel {
            chat: chat.clone(),
            model: ModelId::parse("sonnet").unwrap_or_else(|| unreachable!("a model name")),
        },
        rewind_command(
            chat.clone(),
            (2, Prompt::from("again")),
            10,
            RewindScope::ConversationAndFiles,
        ),
    ];
    let Some(routed) = route_chat(&place, &local_client, &hosted) else {
        unreachable!("a followed runner routes");
    };
    for command in commands.clone() {
        routed.post(Command::Chat(command));
    }
    let sent: Vec<Command> = commands.into_iter().map(Command::Chat).collect();
    assert_eq!(runner.sent(), sent);
    assert_eq!(local.sent(), Vec::<Command>::new());
    assert!(matches!(
        runner.sent().last(),
        Some(Command::Chat(ChatCommand::Rewind {
            index: 12,
            scope: RewindScope::ConversationAndFiles,
            ..
        }))
    ));
    let here = ChatPlace::here(&EnvironmentName::new("main".to_owned()));
    assert!(route_chat(&here, &local_client, &hosted)
        .is_some_and(|routed| Arc::ptr_eq(routed, &local_client)));
}

#[test]
fn a_runner_out_of_reach_routes_nothing() {
    let place = runner_place();
    let local: Arc<dyn RuntimeClient> = RecordedClient::new(Vec::new());
    let mut hosted = HostedPlaces::default();
    hosted.follow(&place, None, None);
    assert!(route_chat(&place, &local, &hosted).is_none());
}

#[test]
fn an_answer_the_runner_already_took_from_another_client_shows_as_a_warning() {
    let refused = Reply::Refused {
        code: ErrorCode::AlreadyAnswered,
        message: "this ask was already answered".to_owned(),
    };
    assert_eq!(
        refusal_toast(refused),
        Some((
            ToastKind::Warning,
            "this ask was already answered".to_owned()
        ))
    );
    assert_eq!(refusal_toast(Reply::Accepted), None);
}
