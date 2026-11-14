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
fn a_new_cloud_session_runs_on_the_runner_s_runtime_only_when_it_is_connected() {
    use super::super::sessions::reach_for;
    use krad_protocol::CloudReach;
    assert_eq!(reach_for(true), CloudReach::Runtime);
    assert_eq!(reach_for(false), CloudReach::ProcessWire);
}
