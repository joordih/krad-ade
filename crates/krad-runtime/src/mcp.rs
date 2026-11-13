use krad_domain::{
    McpFileError, McpProjectFile, McpProjectTrust, McpServer, McpServerName, McpServerStart,
    McpServerState, McpSettings, McpTransport, McpValue, McpVariable, PROJECT_MCP_FILE,
};
use krad_host::HostLink;
use krad_store::find_mcp_secret;
use std::collections::HashMap;
use std::path::Path;

const CODE_TOOLS_SERVER: &str = "krad_code";
const CODE_TOOLS_VERB: &str = "mcp";
const SECRET_PROBLEM_SEPARATOR: &str = ": ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpTrust {
    Trusted(Vec<McpServer>),
    Ignored,
    Unknown(McpProjectFile),
}

#[derive(Clone, Debug, Default)]
pub struct McpLaunch {
    pub starts: Vec<McpServerStart>,
    pub problems: Vec<String>,
}

/// # Errors
///
/// Fails with a readable reason when the project's MCP file exists but cannot be read or parsed.
pub fn read_project_mcp(link: &HostLink, folder: &Path) -> Result<McpProjectFile, String> {
    match link.read_to_string(&folder.join(PROJECT_MCP_FILE)) {
        Ok(text) => text
            .parse()
            .map_err(|error: McpFileError| error.to_string()),
        Err(error) if error.is_not_found() => Ok(McpProjectFile::default()),
        Err(error) => Err(error.to_string()),
    }
}

#[must_use]
pub fn trust_of(stored: Option<McpProjectTrust>, file: McpProjectFile) -> McpTrust {
    match stored {
        Some(McpProjectTrust::Trusted) => McpTrust::Trusted(file.servers),
        Some(McpProjectTrust::Ignored) => McpTrust::Ignored,
        None if file.servers.is_empty() => McpTrust::Trusted(Vec::new()),
        None => McpTrust::Unknown(file),
    }
}

#[must_use]
pub fn trusted_servers(file: McpProjectFile, trust: McpProjectTrust) -> Vec<McpServer> {
    match trust {
        McpProjectTrust::Trusted => file.servers,
        McpProjectTrust::Ignored => Vec::new(),
    }
}

fn settings_servers<'a>(
    settings: &'a McpSettings,
    chosen: Option<&[McpServerName]>,
) -> Vec<&'a McpServer> {
    match chosen {
        Some(names) => settings
            .servers
            .iter()
            .filter(|server| names.contains(&server.name))
            .collect(),
        None => settings.enabled_servers().collect(),
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

#[must_use]
pub fn mcp_servers(
    settings: &McpSettings,
    chosen: Option<&[McpServerName]>,
    project: Vec<McpServer>,
) -> Vec<McpServer> {
    servers_to_start(&settings_servers(settings, chosen), project)
}

#[must_use]
pub fn with_secrets(servers: Vec<McpServer>) -> McpLaunch {
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

#[must_use]
pub fn code_tools_server(bus_vars: Vec<(String, String)>) -> Option<McpServer> {
    if bus_vars.is_empty() {
        return None;
    }
    let env = bus_vars
        .into_iter()
        .filter_map(|(name, value)| {
            Some(McpVariable {
                name: name.parse().ok()?,
                value: McpValue::Plain(value),
            })
        })
        .collect();
    Some(McpServer {
        name: CODE_TOOLS_SERVER.parse().ok()?,
        transport: McpTransport::Stdio {
            command: krad_agents::CLI_PROGRAM.to_owned(),
            args: vec![CODE_TOOLS_VERB.to_owned()],
            env,
        },
        state: McpServerState::Enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let summaries: Vec<String> = started.iter().map(McpServer::summary).collect();
        assert_eq!(summaries, ["mine --stdio", "docs --stdio"]);
    }

    #[test]
    fn a_template_picks_settings_servers_by_name_even_when_disabled() {
        let mut off = server("browser", "browser");
        off.state = McpServerState::Disabled;
        let settings = McpSettings {
            servers: vec![server("files", "files"), off],
            ..McpSettings::default()
        };
        let names = |picked: Vec<&McpServer>| -> Vec<String> {
            picked
                .into_iter()
                .map(|server| server.name.as_str().to_owned())
                .collect()
        };
        assert_eq!(names(settings_servers(&settings, None)), ["files"]);
        let chosen: Vec<McpServerName> = ["browser", "missing"]
            .into_iter()
            .filter_map(|name| name.parse().ok())
            .collect();
        assert_eq!(
            names(settings_servers(&settings, Some(&chosen))),
            ["browser"]
        );
        assert_eq!(
            names(settings_servers(&settings, Some(&[]))),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_folder_without_a_project_file_offers_nothing() {
        let folder = std::env::temp_dir().join(format!("krad-mcp-none-{}", std::process::id()));
        assert_eq!(
            read_project_mcp(&HostLink::Local, &folder),
            Ok(McpProjectFile::default())
        );
    }

    #[test]
    fn the_code_tools_server_carries_the_bus_variables_and_nothing_without_them() {
        assert_eq!(code_tools_server(Vec::new()), None);
        let started = code_tools_server(vec![("KRAD_BUS".to_owned(), "here".to_owned())])
            .unwrap_or_else(|| unreachable!("bus variables make a server"));
        assert_eq!(started.name.as_str(), CODE_TOOLS_SERVER);
        assert_eq!(started.transport.variables().len(), 1);
    }
}
