use super::*;
use crate::agent::{AgentPreset, AgentState};
use crate::archive::fixtures::{now, Workbench};
use crate::archive::ArchiveRetention;
use crate::environment::{NodeKind, TerminalMeta, TerminalNode};
use crate::group::{FileOpening, FileView, SessionLead, SessionLook, WorkerSpawn};
use crate::text_scale::TextScale;
use crate::toolchain::GroupActivation;
use std::path::PathBuf;

struct Bench {
    data: EnvironmentData,
    group: GroupId,
    chat: TerminalId,
    shell: TerminalId,
}

fn bench() -> Bench {
    let mut workbench = Workbench::new();
    let chat = workbench.push(NodeKind::Chat, Some("chat-1"));
    let shell = workbench.push(NodeKind::Terminal, None);
    workbench.data.meta.insert(
        shell,
        TerminalMeta {
            cwd: "/work/api".to_owned(),
            scrollback: vec!["ls".to_owned()],
        },
    );
    Bench {
        data: workbench.data,
        group: workbench.group,
        chat,
        shell,
    }
}

fn node(id: TerminalId, group: GroupId) -> Box<TerminalNode> {
    let mut node = bench().data.nodes[0].clone();
    node.id = id;
    node.group = Some(group);
    node.kind = NodeKind::Chat;
    node.resume = None;
    Box::new(node)
}

fn edits(bench: &Bench) -> Vec<EnvironmentEdit> {
    let data = &bench.data;
    let session = data.active_session.unwrap_or(SessionId(1));
    let fresh = data.next_session_id();
    let worker = TerminalId(data.next_id);
    vec![
        SessionEdit::Open {
            session: fresh,
            group: Some(bench.group),
            name: "Refunds".to_owned(),
            shown: false,
        }
        .into(),
        SessionEdit::Rename {
            session,
            name: "Payments".to_owned(),
        }
        .into(),
        SessionEdit::SetLook {
            session,
            look: SessionLook::default(),
        }
        .into(),
        SessionEdit::Reveal {
            terminal: bench.shell,
        }
        .into(),
        SessionEdit::ToggleAttention { session }.into(),
        SessionEdit::MakeLead {
            session,
            lead: SessionLead::new(AgentPreset::ClaudeCode, None),
        }
        .into(),
        NodeEdit::Open {
            node: node(worker, bench.group),
            place: NodePlace::Worker {
                session: fresh,
                spawn: WorkerSpawn {
                    lead: Some(session),
                    task: "Fix the build".to_owned(),
                },
                cloud: None,
            },
        }
        .into(),
        NodeEdit::Set {
            terminal: bench.chat,
            field: NodeField::State(AgentState::Working),
        }
        .into(),
        NodeEdit::Set {
            terminal: bench.chat,
            field: NodeField::TextScale(TextScale::default().bigger()),
        }
        .into(),
        NodeEdit::OpenFile {
            group: Some(bench.group),
            path: PathBuf::from("/work/api/a.rs"),
            view: FileView::Edit,
            opening: FileOpening::Pinned,
        }
        .into(),
        GroupEdit::Rename {
            group: bench.group,
            name: "api".to_owned(),
        }
        .into(),
        GroupEdit::SetActivation {
            group: bench.group,
            activation: GroupActivation::default(),
        }
        .into(),
        NodeEdit::Close {
            terminal: bench.shell,
        }
        .into(),
        SessionEdit::Archive { session, at: now() }.into(),
        SessionEdit::Close { session }.into(),
        SessionEdit::ExpireArchive {
            retention: ArchiveRetention::default(),
            now: now(),
        }
        .into(),
        GroupEdit::Add {
            group: data.next_group_id(),
            root: PathBuf::from("/work/web"),
            worktree: None,
        }
        .into(),
        GroupEdit::Remove { group: bench.group }.into(),
    ]
}

#[test]
fn every_edit_tells_exactly_the_changes_it_made() {
    let bench = bench();
    let mut edited = bench.data.clone();
    let mut mirror = bench.data.clone();
    for edit in edits(&bench) {
        let (_, changes) = edited.edit_with_changes(&edit);
        for change in changes {
            mirror.apply(change);
        }
        assert_eq!(mirror.nodes, edited.nodes, "{edit:?}");
        assert_eq!(mirror.sessions, edited.sessions, "{edit:?}");
        assert_eq!(mirror.groups, edited.groups, "{edit:?}");
        assert_eq!(mirror.active_session, edited.active_session, "{edit:?}");
        assert_eq!(mirror.next_id, edited.next_id, "{edit:?}");
        assert_eq!(mirror.archive, edited.archive, "{edit:?}");
        assert_eq!(mirror.retired_spend, edited.retired_spend, "{edit:?}");
        assert_eq!(mirror.meta, edited.meta, "{edit:?}");
    }
    assert!(edited.groups.iter().all(|group| group.id != bench.group));
}

#[test]
fn an_edit_applied_on_two_copies_leaves_them_equal() {
    let bench = bench();
    let mut client = bench.data.clone();
    let mut runtime = bench.data.clone();
    for edit in edits(&bench) {
        let told = client.edit(&edit);
        runtime.edit_with_changes(&edit);
        assert_eq!(client, runtime, "{edit:?} told {told:?}");
    }
}

#[test]
fn opening_an_item_whose_id_is_taken_changes_nothing() {
    let bench = bench();
    let mut data = bench.data.clone();
    let taken = NodeEdit::Open {
        node: node(bench.chat, bench.group),
        place: NodePlace::Tab { anchor: None },
    };
    assert_eq!(data.edit(&taken.into()), Edited::Ignored);
    let session = data.active_session.unwrap_or(SessionId(1));
    let again = SessionEdit::Open {
        session,
        group: None,
        name: String::new(),
        shown: true,
    };
    assert!(!data.edit(&again.into()).applied());
    let group = GroupEdit::Add {
        group: bench.group,
        root: PathBuf::from("/elsewhere"),
        worktree: None,
    };
    assert!(!data.edit(&group.into()).applied());
    assert_eq!(data, bench.data);
}

#[test]
fn a_session_opened_unshown_keeps_the_shown_one() {
    let bench = bench();
    let mut data = bench.data.clone();
    let shown = data.active_session;
    let session = data.next_session_id();
    let edit = SessionEdit::Open {
        session,
        group: Some(bench.group),
        name: String::new(),
        shown: false,
    };
    assert!(data.edit(&edit.into()).applied());
    assert_eq!(data.active_session, shown);
    assert!(data.session(session).is_some());
}

#[test]
fn edits_have_a_stable_wire_shape_and_newer_verbs_read_as_unknown() -> serde_json::Result<()> {
    let edit = SessionEdit::Close {
        session: SessionId(3),
    };
    let json = serde_json::to_string(&edit)?;
    assert_eq!(json, r#"{"verb":"close","session":3}"#);
    assert_eq!(serde_json::from_str::<SessionEdit>(&json)?, edit);
    let field = NodeEdit::Set {
        terminal: TerminalId(2),
        field: NodeField::State(AgentState::Working),
    };
    let json = serde_json::to_string(&field)?;
    assert_eq!(serde_json::from_str::<NodeEdit>(&json)?, field);
    let newer: SessionEdit = serde_json::from_str(r#"{"verb":"fly","session":3}"#)?;
    assert_eq!(newer, SessionEdit::Unknown);
    let newer: GroupEdit = serde_json::from_str(r#"{"verb":"fly"}"#)?;
    assert_eq!(newer, GroupEdit::Unknown);
    let newer: NodeEdit = serde_json::from_str(r#"{"verb":"fly"}"#)?;
    assert_eq!(newer, NodeEdit::Unknown);
    Ok(())
}
