use super::*;
use krad_domain::{AgentPreset, SessionLead};

#[test]
fn a_lead_closes_only_the_sessions_the_close_rules_give_it() {
    let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    let api = data.add_group(PathBuf::from("/work/api"), None);
    let web = data.add_group(PathBuf::from("/work/web"), None);
    data.enter_sessions();
    let lead = data.new_session(Some(api), krad_domain::SessionSpec::default());
    if let Some(session) = data.sessions.iter_mut().find(|s| s.id == lead) {
        session.lead = Some(SessionLead::new(AgentPreset::ClaudeCode, None));
    }
    let sibling = data.new_session(Some(api), krad_domain::SessionSpec::default());
    let foreign = data.new_session(Some(web), krad_domain::SessionSpec::default());
    let code = |session| session_close_refusal(&data, lead, session).map(|error| error.code);
    assert_eq!(code(sibling), None);
    assert_eq!(code(foreign), Some(ErrorCode::NotOwned));
    assert_eq!(code(lead), Some(ErrorCode::Forbidden));
    assert_eq!(code(SessionId(999)), Some(ErrorCode::NotFound));
}

#[test]
fn a_lead_picks_only_the_variants_runs_of_its_group() {
    let (mine, theirs) = (GroupId(1), GroupId(2));
    assert_eq!(pick_refusal(Some(mine), mine), None);
    let code = |run_group| pick_refusal(run_group, mine).map(|error| error.code);
    assert_eq!(code(Some(theirs)), Some(ErrorCode::NotOwned));
    assert_eq!(code(None), Some(ErrorCode::NotFound));
}
