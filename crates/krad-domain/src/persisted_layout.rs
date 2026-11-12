use crate::environment::EnvironmentData;
use crate::ids::EnvironmentName;
use crate::sidebar_width::SidebarWidth;
use crate::toolchain::ToolchainChoices;
use crate::work_mode::WorkMode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const LAYOUT_SCHEMA_VERSION: u32 = 12;

pub const SCROLLBACK_PERSIST_LINES: usize = 5000;

#[derive(Clone, Debug, PartialEq)]
pub struct UnreadableEnvironment(serde_json::Value);

#[derive(Debug)]
pub struct PersistedLayout {
    pub version: u32,
    pub active: EnvironmentName,
    pub environments: Vec<EnvironmentData>,
    pub unreadable: Vec<UnreadableEnvironment>,
    pub work_mode: WorkMode,
    pub sidebar_width: SidebarWidth,
    pub toolchains: ToolchainChoices,
}

#[derive(Serialize, Deserialize)]
struct StoredLayout {
    version: u32,
    active: EnvironmentName,
    environments: Vec<serde_json::Value>,
    #[serde(default)]
    work_mode: WorkMode,
    #[serde(default)]
    sidebar_width: SidebarWidth,
    #[serde(default, skip_serializing_if = "ToolchainChoices::is_empty")]
    toolchains: ToolchainChoices,
}

impl Serialize for PersistedLayout {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let readable = self
            .environments
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(serde::ser::Error::custom)?;
        StoredLayout {
            version: self.version,
            active: self.active.clone(),
            environments: readable
                .into_iter()
                .chain(self.unreadable.iter().map(|kept| kept.0.clone()))
                .collect(),
            work_mode: self.work_mode,
            sidebar_width: self.sidebar_width,
            toolchains: self.toolchains.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PersistedLayout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredLayout::deserialize(deserializer)?;
        let mut environments = Vec::new();
        let mut unreadable = Vec::new();
        for raw in stored.environments {
            match EnvironmentData::deserialize(&raw) {
                Ok(environment) => environments.push(environment),
                Err(_) => unreadable.push(UnreadableEnvironment(raw)),
            }
        }
        Ok(Self {
            version: stored.version,
            active: stored.active,
            environments,
            unreadable,
            work_mode: stored.work_mode,
            sidebar_width: stored.sidebar_width,
            toolchains: stored.toolchains,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentPreset;
    use crate::cloud::{CloudPlacement, CloudProcess, CloudRunnerId, JournalOffset};
    use crate::environment::DEFAULT_ENVIRONMENT;
    use crate::group::SessionSpec;
    use crate::host::Host;
    use crate::toolchain::{
        GroupActivation, LocatorId, ProjectFolder, Toolchain, ToolchainFamilyId, ToolchainKind,
        ToolchainLocation, ToolchainScope, ToolchainSelection,
    };
    use std::path::PathBuf;

    #[test]
    fn sessions_roundtrip_with_their_tabs() -> serde_json::Result<()> {
        let mut environment =
            EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()));
        let group = environment.add_group(PathBuf::from("/work/api"), None);
        environment.enter_sessions();
        let session = environment.new_session(Some(group), SessionSpec::default());
        let snapshot = PersistedLayout {
            version: LAYOUT_SCHEMA_VERSION,
            active: environment.name.clone(),
            environments: vec![environment],
            unreadable: Vec::new(),
            work_mode: WorkMode::Editor,
            sidebar_width: SidebarWidth::new(320.0),
            toolchains: ToolchainChoices::default(),
        };
        let back: PersistedLayout = serde_json::from_str(&serde_json::to_string(&snapshot)?)?;
        let Some(env) = back.environments.first() else {
            return Err(serde::de::Error::custom("no environment"));
        };
        assert_eq!(env.sessions, snapshot.environments[0].sessions);
        assert_eq!(env.active_session, Some(session));
        assert_eq!(back.work_mode, WorkMode::Editor);
        Ok(())
    }

    #[test]
    fn an_unreadable_environment_is_kept_verbatim_beside_the_rest() -> serde_json::Result<()> {
        let mut good = EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()));
        good.add_group(PathBuf::from("/work/api"), None);
        let snapshot = PersistedLayout {
            version: LAYOUT_SCHEMA_VERSION,
            active: good.name.clone(),
            environments: vec![good],
            unreadable: Vec::new(),
            work_mode: WorkMode::Editor,
            sidebar_width: SidebarWidth::new(320.0),
            toolchains: ToolchainChoices::default(),
        };
        let mut json = serde_json::to_value(&snapshot)?;
        let mut broken = json["environments"][0].clone();
        broken["name"] = serde_json::Value::from("remote");
        broken["host"] = serde_json::json!({ "kind": "ssh", "destination": "-oProxyCommand=x" });
        if let Some(environments) = json["environments"].as_array_mut() {
            environments.push(broken);
        }
        let back: PersistedLayout = serde_json::from_value(json)?;
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
    fn toolchain_choices_and_group_activation_survive_a_restart() -> serde_json::Result<()> {
        let mut environment =
            EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()));
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
        let snapshot = PersistedLayout {
            version: LAYOUT_SCHEMA_VERSION,
            active: environment.name.clone(),
            environments: vec![environment],
            unreadable: Vec::new(),
            work_mode: WorkMode::Editor,
            sidebar_width: SidebarWidth::new(320.0),
            toolchains,
        };
        let back: PersistedLayout = serde_json::from_str(&serde_json::to_string(&snapshot)?)?;
        assert_eq!(
            back.toolchains.selection(&scope),
            ToolchainSelection::Chosen(Box::new(venv))
        );
        assert_eq!(
            back.environments[0].groups[0].activation,
            GroupActivation::On
        );
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
        let back: PersistedLayout = serde_json::from_value(older)?;
        assert!(back.toolchains.is_empty());
        assert_eq!(
            back.environments[0].groups[0].activation,
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
        let back: PersistedLayout = serde_json::from_value(older)?;
        let harnesses: Vec<Option<AgentPreset>> = back.environments[0]
            .nodes
            .iter()
            .map(|node| node.harness)
            .collect();
        assert_eq!(
            harnesses,
            [Some(AgentPreset::ClaudeCode), Some(AgentPreset::Gateway)]
        );
        let mut snapshot = back;
        snapshot.environments[0].nodes[1].harness = Some(AgentPreset::Acp(
            "codex-acp"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid id")),
        ));
        let text = serde_json::to_string(&snapshot)?;
        assert!(text.contains(r#""harness":"ClaudeCode""#), "{text}");
        assert!(text.contains(r#""harness":"Acp:codex-acp""#), "{text}");
        let again: PersistedLayout = serde_json::from_str(&text)?;
        assert_eq!(
            again.environments[0].nodes[1].harness,
            snapshot.environments[0].nodes[1].harness
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
        };
        let process = CloudProcess {
            id: "chat-1".parse()?,
            offset: JournalOffset(4096),
        };
        if let Some(session) = environment.current_session_mut() {
            session.cloud = Some(placement.clone());
        }
        environment.nodes[0].cloud = Some(process.clone());
        let snapshot = PersistedLayout {
            version: LAYOUT_SCHEMA_VERSION,
            active: environment.name.clone(),
            environments: vec![environment],
            unreadable: Vec::new(),
            work_mode: WorkMode::Editor,
            sidebar_width: SidebarWidth::new(320.0),
            toolchains: ToolchainChoices::default(),
        };
        let text = serde_json::to_string(&snapshot)?;
        assert!(text.contains(r#""offset":4096"#), "{text}");
        let back: PersistedLayout = serde_json::from_str(&text)?;
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
    fn a_layout_written_before_cloud_sessions_loads_and_stays_without_them(
    ) -> serde_json::Result<()> {
        let mut environment = crate::environment::seed_environment();
        let group = environment.add_group(PathBuf::from("/work/api"), None);
        environment.enter_sessions();
        environment.new_session(Some(group), SessionSpec::default());
        let snapshot = PersistedLayout {
            version: LAYOUT_SCHEMA_VERSION,
            active: environment.name.clone(),
            environments: vec![environment],
            unreadable: Vec::new(),
            work_mode: WorkMode::Editor,
            sidebar_width: SidebarWidth::new(320.0),
            toolchains: ToolchainChoices::default(),
        };
        let older = serde_json::to_string(&snapshot)?;
        assert!(!older.contains("cloud"), "{older}");
        let back: PersistedLayout = serde_json::from_str(&older)?;
        let env = &back.environments[0];
        assert_ne!(env.sessions.len(), 0);
        assert!(env.sessions.iter().all(|session| session.cloud.is_none()));
        assert!(env.nodes.iter().all(|node| node.cloud.is_none()));
        Ok(())
    }
}
