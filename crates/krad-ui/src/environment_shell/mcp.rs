mod tray;

use gpui_kit::{AppContext as _, Context, Task};
use krad_domain::{
    McpFileError, McpProjectFile, McpProjectTrust, McpServer, McpServerStart, McpTransport,
    PROJECT_MCP_FILE,
};
use krad_store::find_mcp_secret;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::EnvironmentShell;

const ARGUMENT_JOINER: &str = " ";
const SECRET_PROBLEM_SEPARATOR: &str = ": ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum McpTrust {
    Trusted(Vec<McpServer>),
    Ignored,
    Unknown(McpProjectFile),
}

#[derive(Debug, Default)]
pub(super) struct McpLaunch {
    pub(super) starts: Vec<McpServerStart>,
    pub(super) problems: Vec<String>,
}

pub(super) fn read_project_mcp(folder: &Path) -> Result<McpProjectFile, String> {
    match std::fs::read_to_string(folder.join(PROJECT_MCP_FILE)) {
        Ok(text) => text
            .parse()
            .map_err(|error: McpFileError| error.to_string()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(McpProjectFile::default()),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn trust_of(stored: Option<McpProjectTrust>, file: McpProjectFile) -> McpTrust {
    match stored {
        Some(McpProjectTrust::Trusted) => McpTrust::Trusted(file.servers),
        Some(McpProjectTrust::Ignored) => McpTrust::Ignored,
        None if file.servers.is_empty() => McpTrust::Trusted(Vec::new()),
        None => McpTrust::Unknown(file),
    }
}

pub(super) fn server_summary(server: &McpServer) -> String {
    match &server.transport {
        McpTransport::Stdio { command, args, .. } => std::iter::once(command.as_str())
            .chain(args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(ARGUMENT_JOINER),
        McpTransport::Http { url, .. } => url.to_string(),
    }
}

fn servers_to_start(settings: &[&McpServer], project: Vec<McpServer>) -> Vec<McpServer> {
    let mut servers: Vec<McpServer> = settings.iter().map(|server| (*server).clone()).collect();
    for server in project {
        if !servers.iter().any(|kept| kept.name == server.name) {
            servers.push(server);
        }
    }
    servers
}

fn with_secrets(servers: Vec<McpServer>) -> McpLaunch {
    let mut launch = McpLaunch::default();
    for server in servers {
        let mut secrets = HashMap::new();
        for key in server.secret_keys() {
            match find_mcp_secret(&key) {
                Ok(Some((secret, _))) => {
                    secrets.insert(key.variable, secret);
                }
                Ok(None) => {}
                Err(error) => launch
                    .problems
                    .push(format!("{key}{SECRET_PROBLEM_SEPARATOR}{error}")),
            }
        }
        launch.starts.push(McpServerStart { server, secrets });
    }
    launch
}

impl EnvironmentShell {
    pub(super) fn load_project_mcp(
        folder: PathBuf,
        cx: &mut Context<Self>,
    ) -> Task<Result<McpProjectFile, String>> {
        cx.background_spawn(async move { read_project_mcp(&folder) })
    }

    pub(super) fn mcp_trust_for(&self, folder: &Path, file: McpProjectFile) -> McpTrust {
        trust_of(self.settings.mcp.project_trust(folder), file)
    }

    pub(super) fn set_mcp_project_trust(
        &mut self,
        folder: &Path,
        trust: McpProjectTrust,
        cx: &mut Context<Self>,
    ) {
        self.settings.mcp.set_project_trust(folder, trust);
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn forget_mcp_project(&mut self, folder: &Path, cx: &mut Context<Self>) {
        self.settings.mcp.forget_project(folder);
        self.save_settings(cx);
        cx.notify();
    }

    pub(super) fn mcp_launch(
        &self,
        project: Vec<McpServer>,
        cx: &mut Context<Self>,
    ) -> Task<McpLaunch> {
        let enabled: Vec<&McpServer> = self.settings.mcp.enabled_servers().collect();
        let servers = servers_to_start(&enabled, project);
        cx.background_spawn(async move { with_secrets(servers) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::McpServerState;

    fn server(name: &str, command: &str) -> McpServer {
        McpServer {
            name: name
                .parse()
                .unwrap_or_else(|_| unreachable!("{name} is a server name")),
            transport: McpTransport::Stdio {
                command: command.to_owned(),
                args: vec!["--stdio".to_owned()],
                env: Vec::new(),
            },
            state: McpServerState::Enabled,
        }
    }

    fn file(servers: Vec<McpServer>) -> McpProjectFile {
        McpProjectFile {
            servers,
            skipped: Vec::new(),
        }
    }

    #[test]
    fn a_project_file_asks_once_and_then_follows_the_choice() {
        let offered = file(vec![server("files", "npx")]);
        assert_eq!(
            trust_of(None, offered.clone()),
            McpTrust::Unknown(offered.clone())
        );
        assert_eq!(
            trust_of(Some(McpProjectTrust::Trusted), offered.clone()),
            McpTrust::Trusted(offered.servers.clone())
        );
        assert_eq!(
            trust_of(Some(McpProjectTrust::Ignored), offered),
            McpTrust::Ignored
        );
        assert_eq!(
            trust_of(None, file(Vec::new())),
            McpTrust::Trusted(Vec::new())
        );
    }

    #[test]
    fn a_settings_server_wins_over_a_project_one_with_its_name() {
        let mine = server("files", "mine");
        let started = servers_to_start(
            &[&mine],
            vec![server("files", "theirs"), server("docs", "docs")],
        );
        let summaries: Vec<String> = started.iter().map(server_summary).collect();
        assert_eq!(summaries, ["mine --stdio", "docs --stdio"]);
    }

    #[test]
    fn a_folder_without_a_project_file_offers_nothing() {
        let folder = std::env::temp_dir().join(format!("krad-mcp-none-{}", std::process::id()));
        assert_eq!(read_project_mcp(&folder), Ok(McpProjectFile::default()));
    }
}
