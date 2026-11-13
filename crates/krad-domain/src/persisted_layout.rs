use crate::backdrop_image::Backdrops;
use crate::environment::EnvironmentData;
use crate::ids::EnvironmentName;
use crate::sidebar_width::SidebarWidth;
use crate::toolchain::ToolchainChoices;
use crate::work_mode::WorkMode;
use serde::{Deserialize, Deserializer};

mod state;
mod view_preferences;

pub use state::RuntimeState;
pub use view_preferences::{EnvironmentViewPreferences, ViewPreferences};

pub const LAYOUT_SCHEMA_VERSION: u32 = 13;

pub const LAST_SINGLE_FILE_VERSION: u32 = 12;

pub const SCROLLBACK_PERSIST_LINES: usize = 5000;

#[derive(Clone, Debug, PartialEq)]
pub struct UnreadableEnvironment(serde_json::Value);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub state: RuntimeState,
    pub view: ViewPreferences,
}

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

#[derive(Deserialize)]
struct StoredLayout {
    version: u32,
    active: EnvironmentName,
    environments: Vec<serde_json::Value>,
    #[serde(default)]
    work_mode: WorkMode,
    #[serde(default)]
    sidebar_width: SidebarWidth,
    #[serde(default)]
    toolchains: ToolchainChoices,
}

fn read_with_view(raw: &serde_json::Value) -> Option<EnvironmentData> {
    let mut environment = EnvironmentData::deserialize(raw).ok()?;
    let view = EnvironmentViewPreferences::deserialize(raw).ok()?;
    environment.restore_view(view);
    Some(environment)
}

impl<'de> Deserialize<'de> for PersistedLayout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredLayout::deserialize(deserializer)?;
        let mut environments = Vec::new();
        let mut unreadable = Vec::new();
        for raw in stored.environments {
            match read_with_view(&raw) {
                Some(environment) => environments.push(environment),
                None => unreadable.push(UnreadableEnvironment(raw)),
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

impl From<PersistedLayout> for Layout {
    fn from(mut single: PersistedLayout) -> Self {
        let views = single
            .environments
            .iter_mut()
            .map(EnvironmentData::take_view)
            .collect();
        Self {
            state: RuntimeState {
                version: LAYOUT_SCHEMA_VERSION,
                environments: single.environments,
                unreadable: single.unreadable,
            },
            view: ViewPreferences {
                version: LAYOUT_SCHEMA_VERSION,
                active: Some(single.active),
                work_mode: single.work_mode,
                sidebar_width: single.sidebar_width,
                toolchains: single.toolchains,
                backdrops: Backdrops::default(),
                environments: views,
            },
        }
    }
}

#[cfg(test)]
mod tests;
