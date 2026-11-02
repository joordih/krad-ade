use crate::environment::EnvironmentData;
use crate::ids::EnvironmentName;
use crate::sidebar_width::SidebarWidth;
use crate::work_mode::WorkMode;
use serde::{Deserialize, Serialize};

pub const LAYOUT_SCHEMA_VERSION: u32 = 10;

pub const SCROLLBACK_PERSIST_LINES: usize = 5000;

#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedLayout {
    pub version: u32,
    pub active: EnvironmentName,
    pub environments: Vec<EnvironmentData>,
    #[serde(default)]
    pub work_mode: WorkMode,
    #[serde(default)]
    pub sidebar_width: SidebarWidth,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::DEFAULT_ENVIRONMENT;
    use crate::group::SessionSpec;
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
            work_mode: WorkMode::Editor,
            sidebar_width: SidebarWidth::new(320.0),
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
}
