use super::{McpServer, McpServerName, McpServerState};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpProjectTrust {
    Trusted,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerTaken(pub McpServerName);

impl fmt::Display for McpServerTaken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "There is already a server named {}", self.0)
    }
}

impl std::error::Error for McpServerTaken {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSettings {
    #[serde(
        default,
        deserialize_with = "valid_servers",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub servers: Vec<McpServer>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<PathBuf, McpProjectTrust>,
}

fn valid_servers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<McpServer>, D::Error> {
    let raw = Vec::<Value>::deserialize(deserializer)?;
    let mut servers: Vec<McpServer> = Vec::with_capacity(raw.len());
    for server in raw
        .into_iter()
        .filter_map(|raw| serde_json::from_value::<McpServer>(raw).ok())
    {
        if !servers.iter().any(|kept| kept.name == server.name) {
            servers.push(server);
        }
    }
    Ok(servers)
}

impl McpSettings {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn enabled_servers(&self) -> impl Iterator<Item = &McpServer> {
        self.servers
            .iter()
            .filter(|server| server.state == McpServerState::Enabled)
    }

    /// # Errors
    ///
    /// Fails when another server already has its name.
    pub fn add_server(&mut self, server: McpServer) -> Result<(), McpServerTaken> {
        if self.servers.iter().any(|kept| kept.name == server.name) {
            return Err(McpServerTaken(server.name));
        }
        self.servers.push(server);
        Ok(())
    }

    pub fn remove_server(&mut self, name: &McpServerName) -> Option<McpServer> {
        let at = self.servers.iter().position(|kept| kept.name == *name)?;
        Some(self.servers.remove(at))
    }

    pub fn toggle_server(&mut self, name: &McpServerName) {
        if let Some(server) = self.servers.iter_mut().find(|kept| kept.name == *name) {
            server.state = server.state.toggled();
        }
    }

    #[must_use]
    pub fn project_trust(&self, folder: &Path) -> Option<McpProjectTrust> {
        self.projects.get(folder).copied()
    }

    pub fn set_project_trust(&mut self, folder: &Path, trust: McpProjectTrust) {
        self.projects.insert(folder.to_path_buf(), trust);
    }

    pub fn forget_project(&mut self, folder: &Path) {
        self.projects.remove(folder);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{McpTransport, McpUrl};
    use super::*;

    fn server(name: &str) -> McpServer {
        McpServer {
            name: name
                .parse()
                .unwrap_or_else(|_| unreachable!("{name} is a server name")),
            transport: McpTransport::Http {
                url: "https://mcp.example.test"
                    .parse::<McpUrl>()
                    .unwrap_or_else(|_| unreachable!("valid address")),
                headers: Vec::new(),
            },
            state: McpServerState::Enabled,
        }
    }

    #[test]
    fn names_are_unique_and_disabled_servers_do_not_start() {
        let mut settings = McpSettings::default();
        assert!(settings.add_server(server("docs")).is_ok());
        assert!(settings.add_server(server("docs")).is_err());
        assert!(settings.add_server(server("search")).is_ok());
        settings.toggle_server(&server("docs").name);
        let enabled: Vec<&str> = settings
            .enabled_servers()
            .map(|server| server.name.as_str())
            .collect();
        assert_eq!(enabled, ["search"]);
        assert!(settings.remove_server(&server("docs").name).is_some());
        assert_eq!(settings.servers.len(), 1);
    }

    #[test]
    fn a_folder_choice_is_kept_until_forgotten() -> serde_json::Result<()> {
        let mut settings = McpSettings::default();
        let folder = Path::new("/work/app");
        assert_eq!(settings.project_trust(folder), None);
        settings.set_project_trust(folder, McpProjectTrust::Ignored);
        let back: McpSettings = serde_json::from_str(&serde_json::to_string(&settings)?)?;
        assert_eq!(back.project_trust(folder), Some(McpProjectTrust::Ignored));
        settings.forget_project(folder);
        assert!(settings.is_empty());
        Ok(())
    }

    #[test]
    fn unreadable_or_repeated_servers_are_dropped_not_fatal() -> serde_json::Result<()> {
        let read: McpSettings = serde_json::from_str(
            r#"{"servers":[
                {"name":"bad name","transport":{"type":"stdio","command":"x"}},
                {"name":"ok","transport":{"type":"stdio","command":"x"}},
                {"name":"ok","transport":{"type":"stdio","command":"y"}}
            ]}"#,
        )?;
        assert_eq!(read.servers.len(), 1);
        Ok(())
    }
}
