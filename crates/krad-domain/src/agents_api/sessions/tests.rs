use super::*;
use crate::group::SessionSpec;
use crate::ids::EnvironmentName;
use std::path::PathBuf;

fn environment() -> (EnvironmentData, GroupId) {
    let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    let group = data.add_group(PathBuf::from("/work/api"), None);
    data.enter_sessions();
    (data, group)
}

#[test]
fn a_session_lands_in_the_named_group_or_the_callers() {
    let (mut data, api) = environment();
    let web = data.add_group(PathBuf::from("/work/web"), None);
    let web_name = data.group(web).map(|group| group.name.clone());
    assert_eq!(target_group(&data, Some(api), None), Ok(api));
    assert_eq!(target_group(&data, Some(api), web_name.as_deref()), Ok(web));
    assert_eq!(
        target_group(&data, Some(api), Some("nope")).map_err(|error| error.code),
        Err(ErrorCode::NotFound)
    );
    assert_eq!(
        target_group(&data, None, None).map_err(|error| error.code),
        Err(ErrorCode::Unsupported)
    );
    let shown = data.active_session;
    let created = data.new_session(Some(api), SessionSpec::default());
    data.active_session = shown;
    let sessions = session_infos(&data);
    let listed = sessions
        .iter()
        .find(|session| session.session == created.as_u64());
    assert!(listed.is_some_and(|session| !session.is_current));
    assert!(listed.is_some_and(|session| session.group.is_some()));
    assert_eq!(
        sessions.iter().filter(|session| session.is_current).count(),
        usize::from(shown.is_some())
    );
}
