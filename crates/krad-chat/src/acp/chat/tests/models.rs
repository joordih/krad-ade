use krad_domain::{
    AcpConfigOptionId, AcpConfigValue, AcpModeId, AcpModelId, ChatEvent, ModelId, Prompt,
};
use serde_json::{json, Value};

use super::super::AcpWanted;
use super::fake::{order, start, start_choosing, start_wanting, Started, SESSION};
use super::{fresh, is_ready};
use crate::control::{AcpEvent, LiveEvent};
use crate::live::Order;

fn wanting(model: &str) -> Started {
    start_wanting(&fresh(), Some(AcpModelId(model.to_owned())))
}

fn model_option(current: &str) -> Value {
    json!([
        {"id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": "default",
         "options": [{"value": "default", "name": "Default"}, {"value": "plan", "name": "Plan Mode"}]},
        {"id": "model", "name": "Model", "description": "AI model to use", "category": "model",
         "type": "select", "currentValue": current,
         "_meta": {"air": {"recommended": "opus"}},
         "options": [
            {"value": "default", "name": "Default (recommended)", "description": "Opus 4.7"},
            {"value": "sonnet", "name": "Sonnet"},
            {"value": "opus", "name": "Opus"}
         ]},
        {"id": "ultracode", "name": "Ultracode", "type": "boolean", "currentValue": false}
    ])
}

fn open_session(started: &mut Started, session: &Value) -> Vec<LiveEvent> {
    started.agent.initialized(&json!({}), &json!([]));
    let new = started.agent.expect("session/new");
    started.agent.reply(&new, session);
    started.until(is_ready)
}

fn offered_models(events: &[LiveEvent]) -> Vec<String> {
    events
        .iter()
        .find_map(|event| match event {
            LiveEvent::Acp(AcpEvent::Models(models)) => Some(
                models
                    .available
                    .iter()
                    .map(|model| model.id.0.clone())
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn the_models_of_a_config_option_are_offered_like_a_model_list() {
    let mut started = start(&fresh());
    let events = open_session(
        &mut started,
        &json!({"sessionId": SESSION, "configOptions": model_option("default")}),
    );
    assert_eq!(offered_models(&events), ["default", "sonnet", "opus"]);
    assert!(events.iter().any(|event| matches!(
        event,
        LiveEvent::Chat(ChatEvent::Model(name)) if name == "Default (recommended)"
    )));
}

#[test]
fn the_wanted_model_is_set_through_its_config_option_before_the_first_prompt() {
    let mut started = wanting("opus");
    open_session(
        &mut started,
        &json!({"sessionId": SESSION, "configOptions": model_option("default")}),
    );
    order(&started, Order::Send(Prompt::from("hello")));
    let set = started.agent.expect("session/set_config_option");
    assert_eq!(
        set["params"],
        json!({"sessionId": SESSION, "configId": "model", "value": "opus"})
    );
    assert!(started.agent.sent_nothing());
    started
        .agent
        .reply(&set, &json!({"configOptions": model_option("opus")}));
    let prompt = started.agent.expect("session/prompt");
    assert_eq!(prompt["params"]["sessionId"], SESSION);
}

#[test]
fn the_wanted_model_is_found_by_its_name_and_set_through_set_model() {
    let mut started = wanting("Opus");
    open_session(
        &mut started,
        &json!({"sessionId": SESSION, "models": {"currentModelId": "sonnet",
            "availableModels": [{"modelId": "sonnet", "name": "Sonnet"}, {"modelId": "opus", "name": "Opus"}]}}),
    );
    let set = started.agent.expect("session/set_model");
    assert_eq!(
        set["params"],
        json!({"sessionId": SESSION, "modelId": "opus"})
    );
    started.agent.reply(&set, &json!({}));
    let events = started.until(|event| matches!(event, LiveEvent::Chat(ChatEvent::Model(_))));
    assert!(matches!(
        events.last(),
        Some(LiveEvent::Chat(ChatEvent::Model(name))) if name == "Opus"
    ));
}

#[test]
fn a_preferred_model_the_agent_does_not_offer_is_never_sent_and_the_agent_keeps_its_own() {
    let mut started = wanting("claude-fable-5-2");
    let events = open_session(
        &mut started,
        &json!({"sessionId": SESSION, "configOptions": model_option("default")}),
    );
    assert!(!events
        .iter()
        .any(|event| matches!(event, LiveEvent::Notice(_))));
    order(&started, Order::Send(Prompt::from("hello")));
    let prompt = started.agent.expect("session/prompt");
    assert_eq!(prompt["params"]["sessionId"], SESSION);
    started
        .agent
        .reply(&prompt, &json!({"stopReason": "end_turn"}));
    let later = started.until(|event| matches!(event, LiveEvent::Chat(ChatEvent::Done)));
    assert!(!later
        .iter()
        .any(|event| matches!(event, LiveEvent::Notice(_))));
}

#[test]
fn a_model_picked_in_the_chat_that_the_agent_does_not_offer_is_noticed_and_not_sent() {
    let mut started = start(&fresh());
    open_session(
        &mut started,
        &json!({"sessionId": SESSION, "configOptions": model_option("default")}),
    );
    order(
        &started,
        Order::Model(
            ModelId::parse("claude-haiku-9").unwrap_or_else(|| unreachable!("a model id")),
        ),
    );
    let noticed = started.until(|event| matches!(event, LiveEvent::Notice(_)));
    assert!(matches!(
        noticed.last(),
        Some(LiveEvent::Notice(text)) if text.ends_with("claude-haiku-9")
    ));
    order(&started, Order::Send(Prompt::from("hello")));
    started.agent.expect("session/prompt");
}

#[test]
fn the_model_already_running_is_not_set_again() {
    let mut started = wanting("default");
    open_session(
        &mut started,
        &json!({"sessionId": SESSION, "configOptions": model_option("default")}),
    );
    order(&started, Order::Send(Prompt::from("hello")));
    started.agent.expect("session/prompt");
}

#[test]
fn a_model_picked_in_the_chat_is_sent_to_the_open_session() {
    let mut started = start(&fresh());
    open_session(
        &mut started,
        &json!({"sessionId": SESSION, "configOptions": model_option("default")}),
    );
    order(
        &started,
        Order::Model(ModelId::parse("sonnet").unwrap_or_else(|| unreachable!("a model id"))),
    );
    let set = started.agent.expect("session/set_config_option");
    assert_eq!(set["params"]["value"], "sonnet");
}

fn effort_and_mode(effort: &str, mode: &str) -> Value {
    json!([
        {"id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": mode,
         "options": [{"value": "default", "name": "Default"}, {"value": "plan", "name": "Plan Mode"}]},
        {"id": "effort", "name": "Effort", "category": "thought_level", "type": "select",
         "currentValue": effort,
         "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}
    ])
}

#[test]
fn the_mode_and_options_chosen_before_the_session_opened_are_set_before_the_first_prompt() {
    let wanted = AcpWanted {
        mode: Some(AcpModeId("plan".to_owned())),
        options: vec![
            (
                AcpConfigOptionId("effort".to_owned()),
                AcpConfigValue("high".to_owned()),
            ),
            (
                AcpConfigOptionId("effort".to_owned()),
                AcpConfigValue("max".to_owned()),
            ),
        ],
    };
    let mut started = start_choosing(&fresh(), None, wanted);
    let events = open_session(
        &mut started,
        &json!({"sessionId": SESSION, "configOptions": effort_and_mode("low", "default")}),
    );
    assert!(events.iter().any(|event| matches!(
        event,
        LiveEvent::Acp(AcpEvent::Options(options)) if options[0].current.0 == "high"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        LiveEvent::Acp(AcpEvent::Modes(modes)) if modes.current.0 == "plan"
    )));
    order(&started, Order::Send(Prompt::from("hello")));
    let mode = started.agent.expect("session/set_config_option");
    assert_eq!(
        mode["params"],
        json!({"sessionId": SESSION, "configId": "mode", "value": "plan"})
    );
    let effort = started.agent.expect("session/set_config_option");
    assert_eq!(
        effort["params"],
        json!({"sessionId": SESSION, "configId": "effort", "value": "high"})
    );
    started.agent.reply(
        &mode,
        &json!({"configOptions": effort_and_mode("low", "plan")}),
    );
    let settling = started.until(|event| matches!(event, LiveEvent::Acp(AcpEvent::Options(_))));
    assert!(
        matches!(settling.last(), Some(LiveEvent::Acp(AcpEvent::Options(options))) if options[0].current.0 == "high"),
        "the effort still on its way is not shown going back"
    );
    assert!(started.agent.sent_nothing());
    started.agent.reply(
        &effort,
        &json!({"configOptions": effort_and_mode("high", "plan")}),
    );
    let prompt = started.agent.expect("session/prompt");
    assert_eq!(prompt["params"]["sessionId"], SESSION);
}
