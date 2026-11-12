use super::*;
use krad_domain::{AgentPreset, ResumeId, SessionLead, WorkerSpawn};
use std::collections::HashSet;

#[test]
fn every_refusal_answers_with_its_own_stable_code() {
    let refusals = [
        (CloseRefusal::Unknown, ErrorCode::NotFound),
        (CloseRefusal::AlreadyClosed, ErrorCode::NotFound),
        (CloseRefusal::NotOwned, ErrorCode::NotOwned),
        (CloseRefusal::Itself, ErrorCode::Forbidden),
        (CloseRefusal::Working, ErrorCode::Busy),
    ];
    let mut messages = HashSet::new();
    for (refusal, code) in refusals {
        let error = refusal_error(refusal);
        assert_eq!(error.code, code, "{refusal:?}");
        assert!(messages.insert(error.message), "{refusal:?}");
    }
    assert!(refusal_error(CloseRefusal::Working)
        .message
        .contains("--force"));
}

#[test]
fn a_worktree_with_work_left_is_refused_and_other_failures_say_so() {
    assert_eq!(removal_outcome(Ok(())), WorktreeOutcome::Removed);
    for unsettled in [RepoError::UncommittedChanges, RepoError::UnmergedCommits(2)] {
        assert!(matches!(
            removal_outcome(Err(unsettled)),
            WorktreeOutcome::Refused(ApiError {
                code: ErrorCode::WorktreeNotClean,
                ..
            })
        ));
    }
    let failed = removal_outcome(Err(RepoError::Unsupported("x".to_owned())));
    assert!(matches!(
        failed,
        WorktreeOutcome::Refused(ApiError {
            code: ErrorCode::Failed,
            ..
        })
    ));
}

#[test]
fn closed_workers_are_listed_only_as_closed() {
    let mut data = EnvironmentData::empty(EnvironmentName::new("main".to_owned()));
    let api = data.add_group(PathBuf::from("/work/api"), None);
    data.enter_sessions();
    let lead = data.new_session(Some(api), krad_domain::SessionSpec::default());
    if let Some(session) = data.sessions.iter_mut().find(|s| s.id == lead) {
        session.lead = Some(SessionLead::new(AgentPreset::ClaudeCode, None));
    }
    let worker = TerminalId(data.next_id);
    let node = krad_domain::TerminalNode {
        id: worker,
        title: "Claude Code · worker".to_owned(),
        state: AgentState::Done,
        harness: Some(AgentPreset::ClaudeCode),
        resume: "8d0d5ac1-7a3e-4f26-9b1c-2f4b3f1c9a10"
            .parse::<ResumeId>()
            .ok(),
        group: Some(api),
        text_scale: krad_domain::TextScale::default(),
        kind: krad_domain::NodeKind::default(),
        file: None,
        extension: None,
        bookmarks: krad_domain::ChatBookmarks::default(),
        usage: krad_domain::ChatUsage::default(),
        checkpoints: krad_domain::ChatCheckpoints::default(),
        choice: krad_domain::ChatChoice::default(),
        cloud: None,
    };
    data.nodes.push(node);
    let spawn = WorkerSpawn {
        lead: Some(lead),
        task: "Fix the webhook retries\nwith tests".to_owned(),
    };
    data.open_worker_session(worker, spawn);
    assert_eq!(closed_worker_infos(&data), Vec::<AgentInfo>::new());
    data.drop_terminal(worker);
    let listed = closed_worker_infos(&data);
    assert_eq!(
        listed,
        vec![AgentInfo {
            environment: Some(data.name.as_str().to_owned()),
            terminal: worker.as_u64(),
            title: "Fix the webhook retries".to_owned(),
            group: data.group(api).map(|group| group.name.clone()),
            state: TerminalState::Closed,
            harness: Some(AgentPreset::ClaudeCode),
            is_self: false,
        }]
    );
}

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
