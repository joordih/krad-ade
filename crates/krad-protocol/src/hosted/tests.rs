use super::*;
use krad_domain::{
    AgentPreset, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatUsage, CloudPlacement,
    CloudProcess, CloudRunnerId, GroupId, JournalOffset, NodeEdit, NodeKind, NodePlace,
    SessionEdit, SessionSpec, TerminalNode, TextScale,
};
use std::path::PathBuf;

fn chat_node(id: TerminalId, group: GroupId) -> Result<TerminalNode, Box<dyn std::error::Error>> {
    Ok(TerminalNode {
        id,
        title: "Claude".to_owned(),
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
        cloud: Some(CloudProcess {
            id: "fix-1-abcd".parse()?,
            offset: JournalOffset(0),
        }),
    })
}

fn hosted_environment() -> Result<(EnvironmentData, TerminalId), Box<dyn std::error::Error>> {
    let mut data = EnvironmentData::empty(EnvironmentName::new("runner".to_owned()));
    let group = data.add_group(PathBuf::from("/runner/repos/api"), None);
    data.enter_sessions();
    let session = data.new_session(Some(group), SessionSpec::default());
    let placement = CloudPlacement {
        runner: CloudRunnerId(0),
        repo_dir: PathBuf::from("/runner/repos/api"),
        worktree: PathBuf::from("/runner/worktrees/api/fix"),
        branch: "krad/fix".to_owned(),
        origin: Some(HostedOrigin {
            environment: EnvironmentName::new("laptop".to_owned()),
            group: GroupId(4),
        }),
    };
    let cloud = Some(placement);
    data.edit(&SessionEdit::SetCloud { session, cloud }.into());
    let terminal = TerminalId(data.next_id);
    let node = chat_node(terminal, group)?;
    data.edit(
        &NodeEdit::Open {
            node: Box::new(node),
            place: NodePlace::Tab { anchor: None },
        }
        .into(),
    );
    Ok((data, terminal))
}

#[test]
fn a_hosted_session_names_its_origin_branch_and_each_chat_s_live_facts(
) -> Result<(), Box<dyn std::error::Error>> {
    let (data, terminal) = hosted_environment()?;
    let mut state = ChatState::new(AgentPreset::ClaudeCode);
    state.model = Some("opus".to_owned());
    state.effort = Some(Effort::High);
    let facts = |asked: TerminalId| {
        (asked == terminal).then_some(ChatFacts {
            state: &state,
            asks: 1,
            last_activity: Some(SystemTime::UNIX_EPOCH),
            seq: Some(Seq(9)),
        })
    };
    let summaries = hosted_summaries(&data, facts);
    let [summary] = summaries.as_slice() else {
        return Err("one hosted session".into());
    };
    assert_eq!(summary.branch, "krad/fix");
    assert_eq!(
        summary.origin.as_ref().map(|origin| origin.group),
        Some(GroupId(4))
    );
    let [chat] = summary.chats.as_slice() else {
        return Err("one hosted chat".into());
    };
    assert_eq!(chat.target.terminal, terminal);
    assert_eq!(chat.target.environment.as_str(), "laptop");
    assert_eq!(chat.target.state, AgentState::WaitingForInput);
    assert_eq!(chat.model.as_deref(), Some("opus"));
    assert_eq!(chat.effort, Some(Effort::High));
    assert_eq!(summary.seq, Some(Seq(9)));
    assert_eq!(summary.state(), AgentState::WaitingForInput);
    let json = serde_json::to_string(summary)?;
    assert_eq!(&serde_json::from_str::<HostedSummary>(&json)?, summary);
    Ok(())
}

#[test]
fn a_session_that_does_not_run_in_the_cloud_is_not_hosted() {
    let mut data = EnvironmentData::empty(EnvironmentName::new("runner".to_owned()));
    let group = data.add_group(PathBuf::from("/runner/repos/api"), None);
    data.enter_sessions();
    data.new_session(Some(group), SessionSpec::default());
    assert_eq!(hosted_summaries(&data, |_| None), Vec::new());
}
