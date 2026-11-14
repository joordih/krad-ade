use super::*;
use crate::agent::AgentPreset;
use crate::cloud::{CloudPlacement, CloudProcess, CloudRunnerId, JournalOffset};
use crate::environment::DEFAULT_ENVIRONMENT;
use crate::group::SessionSpec;
use crate::host::Host;
use crate::ids::TerminalId;
use crate::toolchain::{
    GroupActivation, LocatorId, ProjectFolder, Toolchain, ToolchainFamilyId, ToolchainKind,
    ToolchainLocation, ToolchainScope, ToolchainSelection,
};
use crate::view::EnvironmentView;
use std::path::PathBuf;

fn main_environment() -> EnvironmentData {
    EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()))
}

fn state_of(environments: Vec<EnvironmentData>) -> RuntimeState {
    RuntimeState {
        environments,
        ..RuntimeState::default()
    }
}

fn through_json<T: serde::Serialize + serde::de::DeserializeOwned>(
    value: &T,
) -> serde_json::Result<T> {
    serde_json::from_str(&serde_json::to_string(value)?)
}

#[test]
fn the_runtime_state_keeps_sessions_and_leaves_the_view_out() -> serde_json::Result<()> {
    let mut environment = main_environment();
    let group = environment.add_group(PathBuf::from("/work/api"), None);
    environment.enter_sessions();
    let session = environment.new_session(Some(group), SessionSpec::default());
    environment.view = EnvironmentView::Notes;
    environment.focus = Some(TerminalId(1));
    environment.side_panel.open = true;
    let state = state_of(vec![environment]);
    let text = serde_json::to_string(&state)?;
    for view_field in ["\"view\"", "\"focus\"", "\"side_panel\""] {
        assert!(!text.contains(view_field), "{text}");
    }
    let back: RuntimeState = serde_json::from_str(&text)?;
    let env = &back.environments[0];
    assert_eq!(env.sessions, state.environments[0].sessions);
    assert_eq!(env.active_session, Some(session));
    assert_eq!(env.view, EnvironmentView::Sessions);
    Ok(())
}

#[test]
fn an_unreadable_environment_is_kept_verbatim_beside_the_rest() -> serde_json::Result<()> {
    let mut good = main_environment();
    good.add_group(PathBuf::from("/work/api"), None);
    let mut json = serde_json::to_value(state_of(vec![good]))?;
    let mut broken = json["environments"][0].clone();
    broken["name"] = serde_json::Value::from("remote");
    broken["host"] = serde_json::json!({ "kind": "ssh", "destination": "-oProxyCommand=x" });
    if let Some(environments) = json["environments"].as_array_mut() {
        environments.push(broken);
    }
    let back: RuntimeState = serde_json::from_value(json)?;
    assert_eq!(back.environments.len(), 1);
    assert_eq!(back.environments[0].groups.len(), 1);
    assert_eq!(back.unreadable.len(), 1);
    let rewritten = serde_json::to_value(&back)?;
    assert_eq!(
        rewritten["environments"][1]["host"]["destination"],
        "-oProxyCommand=x"
    );
    Ok(())
}

#[test]
fn view_preferences_dress_their_environment_after_a_restart() -> serde_json::Result<()> {
    let mut shown = main_environment();
    shown.view = EnvironmentView::Review;
    shown.focus = Some(TerminalId(3));
    shown.side_panel.open = true;
    let view = ViewPreferences {
        active: Some(shown.name.clone()),
        work_mode: WorkMode::Editor,
        sidebar_width: SidebarWidth::new(320.0),
        environments: vec![EnvironmentViewPreferences::from(&shown)],
        ..ViewPreferences::default()
    };
    let mut back = through_json(&view)?;
    assert_eq!(back, view);
    let mut restored = main_environment();
    back.dress(&mut restored);
    assert_eq!(restored, shown);
    assert_eq!(back.environments, Vec::new());
    Ok(())
}

#[test]
fn toolchain_choices_and_group_activation_survive_a_restart() -> serde_json::Result<()> {
    let mut environment = main_environment();
    let group = environment.add_group(PathBuf::from("/work/api"), None);
    if let Some(found) = environment
        .groups
        .iter_mut()
        .find(|found| found.id == group)
    {
        found.activation = GroupActivation::On;
    }
    let scope = ToolchainScope {
        host: Host::Local,
        group_root: PathBuf::from("/work/api"),
        project_folder: ProjectFolder::root(),
        family: ToolchainFamilyId::new("python"),
    };
    let venv = Toolchain {
        family: ToolchainFamilyId::new("python"),
        kind: ToolchainKind::new("venv"),
        name: ".venv".to_owned(),
        version: "3.12.4".parse().ok(),
        location: ToolchainLocation::Path {
            path: PathBuf::from("/work/api/.venv/bin/python"),
        },
        prefix: None,
        project: None,
        locator: LocatorId::new("pet"),
    };
    let mut toolchains = ToolchainChoices::default();
    toolchains.choose(scope.clone(), venv.clone());
    let view = ViewPreferences {
        toolchains,
        ..ViewPreferences::default()
    };
    let back_view = through_json(&view)?;
    let back_state = through_json(&state_of(vec![environment]))?;
    assert_eq!(
        back_view.toolchains.selection(&scope),
        ToolchainSelection::Chosen(Box::new(venv))
    );
    assert_eq!(
        back_state.environments[0].groups[0].activation,
        GroupActivation::On
    );
    Ok(())
}

#[test]
fn a_single_file_layout_splits_into_state_and_view_without_losing_anything(
) -> serde_json::Result<()> {
    let single: PersistedLayout = serde_json::from_value(serde_json::json!({
        "version": 12,
        "active": "remote",
        "work_mode": "Editor",
        "sidebar_width": 288.5,
        "environments": [
            {
                "name": "main",
                "nodes": [{ "id": 4, "title": "claude", "state": "Working", "harness": "ClaudeCode", "group": 1, "text_scale": 1.25 }],
                "meta": { "9": { "cwd": "/work/api", "scrollback": ["$ cargo test"] } },
                "next_id": 10,
                "view": "Notes",
                "focus": 4,
                "title": "Payments",
                "groups": [{ "id": 1, "name": "api", "root": "/work/api" }],
                "side_panel": { "open": true, "width": 401.5, "terminals": [{ "id": 9, "title": "zsh" }], "shown": 9 },
                "note_tabs": [{ "group": 1, "tabs": [{ "path": "plan.md", "pinned": true }], "active": 0 }]
            },
            {
                "name": "remote",
                "host": { "kind": "ssh", "destination": "dev@box" },
                "nodes": [],
                "meta": {},
                "next_id": 1
            },
            {
                "name": "broken",
                "host": { "kind": "ssh", "destination": "-oProxyCommand=x" },
                "nodes": [],
                "meta": {},
                "next_id": 1
            }
        ]
    }))?;
    let whole = single.environments.clone();
    let layout = Layout::from(single);
    let state = through_json(&layout.state)?;
    let mut view = through_json(&layout.view)?;
    assert_eq!(view.active, Some(EnvironmentName::new("remote".to_owned())));
    assert_eq!(view.work_mode, WorkMode::Editor);
    assert_eq!(view.sidebar_width, SidebarWidth::new(288.5));
    assert_eq!(state.unreadable, layout.state.unreadable);
    assert_eq!(state.unreadable.len(), 1);
    let rejoined: Vec<EnvironmentData> = state
        .environments
        .into_iter()
        .map(|mut environment| {
            view.dress(&mut environment);
            environment
        })
        .collect();
    assert_eq!(rejoined, whole);
    assert_eq!(rejoined[0].view, EnvironmentView::Notes);
    assert!(rejoined[0].side_panel.open);
    Ok(())
}

#[test]
fn a_layout_written_before_toolchains_still_loads() -> serde_json::Result<()> {
    let older = serde_json::json!({
        "version": 11,
        "active": "main",
        "environments": [{
            "name": "main",
            "nodes": [],
            "meta": {},
            "next_id": 1,
            "groups": [{ "id": 1, "name": "api", "root": "/work/api" }]
        }]
    });
    let back = Layout::from(serde_json::from_value::<PersistedLayout>(older)?);
    assert!(back.view.toolchains.is_empty());
    assert_eq!(
        back.state.environments[0].groups[0].activation,
        GroupActivation::Inherit
    );
    Ok(())
}

#[test]
fn a_layout_written_before_acp_agents_keeps_its_harnesses() -> serde_json::Result<()> {
    let older = serde_json::json!({
        "version": 11,
        "active": "main",
        "environments": [{
            "name": "main",
            "nodes": [
                { "id": 1, "title": "claude", "state": "Idle", "harness": "ClaudeCode" },
                { "id": 2, "title": "chat", "state": "Done", "harness": "Gateway", "kind": "Chat" }
            ],
            "meta": {},
            "next_id": 3
        }]
    });
    let mut state = Layout::from(serde_json::from_value::<PersistedLayout>(older)?).state;
    let harnesses: Vec<Option<AgentPreset>> = state.environments[0]
        .nodes
        .iter()
        .map(|node| node.harness)
        .collect();
    assert_eq!(
        harnesses,
        [Some(AgentPreset::ClaudeCode), Some(AgentPreset::Gateway)]
    );
    state.environments[0].nodes[1].harness = Some(AgentPreset::Acp(
        "codex-acp"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid id")),
    ));
    let text = serde_json::to_string(&state)?;
    assert!(text.contains(r#""harness":"ClaudeCode""#), "{text}");
    assert!(text.contains(r#""harness":"Acp:codex-acp""#), "{text}");
    let again: RuntimeState = serde_json::from_str(&text)?;
    assert_eq!(
        again.environments[0].nodes[1].harness,
        state.environments[0].nodes[1].harness
    );
    Ok(())
}

#[test]
fn a_cloud_session_keeps_its_runner_and_each_chat_its_journal_offset(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut environment = crate::environment::seed_environment();
    let group = environment.add_group(PathBuf::from("/work/api"), None);
    environment.enter_sessions();
    environment.new_session(Some(group), SessionSpec::default());
    let placement = CloudPlacement {
        runner: CloudRunnerId(7),
        repo_dir: PathBuf::from("/home/me/.krad-ade/runner/repos/api"),
        worktree: PathBuf::from("/home/me/.krad-ade/runner/worktrees/api/fix"),
        branch: "krad/fix".to_owned(),
        origin: Some(crate::cloud::HostedOrigin {
            environment: crate::EnvironmentName::new("laptop".to_owned()),
            group,
        }),
    };
    let process = CloudProcess {
        id: "chat-1".parse()?,
        offset: JournalOffset(4096),
    };
    if let Some(session) = environment.current_session_mut() {
        session.cloud = Some(placement.clone());
    }
    environment.nodes[0].cloud = Some(process.clone());
    let text = serde_json::to_string(&state_of(vec![environment]))?;
    assert!(text.contains(r#""offset":4096"#), "{text}");
    let back: RuntimeState = serde_json::from_str(&text)?;
    let env = &back.environments[0];
    assert_eq!(
        env.current_session()
            .and_then(|session| session.cloud.clone()),
        Some(placement)
    );
    assert_eq!(env.nodes[0].cloud, Some(process));
    assert_eq!(env.nodes[1].cloud, None);
    Ok(())
}

#[test]
fn a_state_written_before_cloud_sessions_loads_and_stays_without_them() -> serde_json::Result<()> {
    let mut environment = crate::environment::seed_environment();
    let group = environment.add_group(PathBuf::from("/work/api"), None);
    environment.enter_sessions();
    environment.new_session(Some(group), SessionSpec::default());
    let older = serde_json::to_string(&state_of(vec![environment]))?;
    assert!(!older.contains("cloud"), "{older}");
    let back: RuntimeState = serde_json::from_str(&older)?;
    let env = &back.environments[0];
    assert_ne!(env.sessions.len(), 0);
    assert!(env.sessions.iter().all(|session| session.cloud.is_none()));
    assert!(env.nodes.iter().all(|node| node.cloud.is_none()));
    Ok(())
}
