use super::*;
use krad_domain::{SessionId, SessionLook};

fn summary(environment: &str, group: u64) -> HostedSummary {
    HostedSummary {
        environment: EnvironmentName::new("runner".to_owned()),
        session: SessionId(1),
        name: "Cloud · api".to_owned(),
        look: SessionLook::default(),
        origin: Some(krad_domain::HostedOrigin {
            environment: EnvironmentName::new(environment.to_owned()),
            group: GroupId(group),
        }),
        branch: "krad/api-1".to_owned(),
        chats: Vec::new(),
        spend: krad_domain::Spend::default(),
        last_activity: None,
        seq: None,
    }
}

#[test]
fn a_hosted_session_shows_only_under_the_group_it_came_from() {
    let here = summary("main", 2);
    let elsewhere = summary("main", 5);
    let other_window = summary("laptop", 2);
    let hosted = [
        (CloudRunnerId(1), &here),
        (CloudRunnerId(1), &elsewhere),
        (CloudRunnerId(2), &other_window),
    ];
    let main = EnvironmentName::new("main".to_owned());
    let shown = hosted_in(hosted.into_iter(), &main, Some(GroupId(2)));
    assert_eq!(shown, [(CloudRunnerId(1), &here)]);
    assert_eq!(hosted_in(hosted.into_iter(), &main, None).len(), 0);
}

#[test]
fn the_runner_is_asked_for_a_hosted_chat_s_models_only_when_none_are_known() {
    let runner = EnvironmentName::new("runner".to_owned());
    let asked = model_listing(false, &runner, AgentPreset::ClaudeCode);
    assert_eq!(
        asked,
        Some(Command::Models(ModelsCommand::List {
            spot: HarnessSpot::at(runner.clone(), None),
            harness: AgentPreset::ClaudeCode,
        }))
    );
    assert_eq!(model_listing(true, &runner, AgentPreset::ClaudeCode), None);
    assert_eq!(model_listing(false, &runner, AgentPreset::Gateway), None);
}
