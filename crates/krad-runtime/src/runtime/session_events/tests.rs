use super::*;
use std::path::PathBuf;

#[test]
fn the_environments_the_runtime_starts_with_publish_their_first_new_session() {
    let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    let group = data.add_group(PathBuf::from("/work/api"), None);
    data.nodes = vec![serde_json::from_value(serde_json::json!({
        "id": 1, "title": "agent", "state": "Idle", "group": group.0
    }))
    .unwrap_or_else(|_| unreachable!("a node"))];
    let mut seen = SessionsSeen::new(std::iter::once(&data));
    data.enter_sessions();
    let moved = seen.moved(std::iter::once(&data));
    assert_eq!(moved.len(), data.sessions.len());
    assert!(moved.iter().all(|(environment, in_group, event)| {
        *environment == data.name
            && *in_group == Some(group)
            && matches!(event, AppEvent::SessionOpened { .. })
    }));
    assert_ne!(moved, Vec::new());
}

#[test]
fn sessions_that_appear_open_and_those_that_vanish_close() {
    let before = [(SessionId(1), Some(GroupId(1))), (SessionId(2), None)];
    let now = [(SessionId(2), None), (SessionId(3), Some(GroupId(1)))];
    assert_eq!(
        session_events(&before, &now),
        vec![
            (
                Some(GroupId(1)),
                AppEvent::SessionOpened {
                    session: SessionId(3)
                }
            ),
            (
                Some(GroupId(1)),
                AppEvent::SessionClosed {
                    session: SessionId(1)
                }
            ),
        ]
    );
    assert_eq!(session_events(&now, &now), Vec::new());
}

#[test]
fn an_environment_opened_later_publishes_nothing_and_then_its_closed_sessions() {
    let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    let group = data.add_group(PathBuf::from("/work/api"), None);
    data.nodes = vec![serde_json::from_value(serde_json::json!({
        "id": 1, "title": "agent", "state": "Idle", "group": group.0
    }))
    .unwrap_or_else(|_| unreachable!("a node"))];
    data.enter_sessions();
    let mut seen = SessionsSeen::new(std::iter::empty());
    assert_eq!(seen.moved(std::iter::once(&data)), Vec::new());
    let first = data.sessions.first().map(|session| session.id);
    data.sessions.clear();
    let moved = seen.moved(std::iter::once(&data));
    let closed: Vec<AppEvent> = moved.into_iter().map(|(_, _, event)| event).collect();
    assert_eq!(
        closed,
        first
            .map(|session| AppEvent::SessionClosed { session })
            .into_iter()
            .collect::<Vec<_>>()
    );
    assert_eq!(seen.moved(std::iter::empty()), Vec::new());
    assert!(seen.0.is_empty());
}
