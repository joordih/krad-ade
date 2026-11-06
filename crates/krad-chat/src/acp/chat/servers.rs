use krad_domain::{
    expand_variables, AcpAgentStart, McpSecret, McpServerStart, McpServerState, McpTransport,
    McpValue, McpVariable, McpVariableName,
};
use krad_host::{HostLink, RemoteLink};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::process::Runner;

use crate::acp::setup::{
    EnvVariable, HttpHeader, McpCapabilities, McpServer, RemoteMcpServer, StdioMcpServer,
};

#[derive(Debug, Error, PartialEq, Eq)]
#[error("the secret {variable} of {owner} is not saved; add it again in Settings")]
pub struct MissingSecret {
    owner: String,
    variable: McpVariableName,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct HandedServers {
    pub(super) servers: Vec<McpServer>,
    pub(super) skipped: Vec<String>,
}

#[derive(Clone, Copy)]
pub enum Variables<'a> {
    Here,
    OnHost(&'a RemoteLink),
}

impl<'a> From<&'a HostLink> for Variables<'a> {
    fn from(link: &'a HostLink) -> Self {
        match link {
            HostLink::Remote(ssh) => Self::OnHost(ssh),
            HostLink::Local => Self::Here,
        }
    }
}

impl<'a> From<&'a Runner> for Variables<'a> {
    fn from(runner: &'a Runner) -> Self {
        match runner {
            Runner::Ssh { link, .. } => Self::from(link),
            Runner::Native(_) | Runner::Wsl { .. } => Self::Here,
        }
    }
}

impl Variables<'_> {
    fn value(self, name: &str) -> Option<String> {
        let found = match self {
            Self::Here => std::env::var(name).ok(),
            Self::OnHost(ssh) => ssh.login_var(name),
        };
        if found.is_none() {
            tracing::debug!(variable = name, "an MCP variable is not set where it runs");
        }
        found
    }

    fn expanded(self, text: &str) -> String {
        expand_variables(text, |name| self.value(name)).into_owned()
    }
}

fn resolved(
    owner: &str,
    variables: &[McpVariable],
    secrets: &HashMap<McpVariableName, McpSecret>,
    from: Variables<'_>,
) -> Result<Vec<(String, String)>, MissingSecret> {
    variables
        .iter()
        .map(|variable| {
            let value = match &variable.value {
                McpValue::Plain(text) => from.expanded(text),
                McpValue::Secret => secrets
                    .get(&variable.name)
                    .map(|secret| secret.expose().to_owned())
                    .ok_or_else(|| MissingSecret {
                        owner: owner.to_owned(),
                        variable: variable.name.clone(),
                    })?,
            };
            Ok((variable.name.as_str().to_owned(), value))
        })
        .collect()
}

/// # Errors
///
/// Fails when a secret value of the agent's env is not among its read secrets.
pub fn acp_agent_env(
    start: &AcpAgentStart,
    from: Variables<'_>,
) -> Result<Vec<(String, String)>, MissingSecret> {
    resolved(&start.agent.name, &start.agent.env, &start.secrets, from)
}

fn handed(
    start: &McpServerStart,
    mcp: McpCapabilities,
    from: Variables<'_>,
) -> Result<Option<McpServer>, MissingSecret> {
    let owner = start.server.name.to_string();
    Ok(Some(match &start.server.transport {
        McpTransport::Stdio { command, args, env } => McpServer::Stdio(StdioMcpServer {
            name: owner.clone(),
            command: PathBuf::from(from.expanded(command)),
            args: args.iter().map(|arg| from.expanded(arg)).collect(),
            env: resolved(&owner, env, &start.secrets, from)?
                .into_iter()
                .map(|(name, value)| EnvVariable { name, value })
                .collect(),
        }),
        McpTransport::Http { .. } if !mcp.http => return Ok(None),
        McpTransport::Http { url, headers } => McpServer::Http(RemoteMcpServer {
            name: owner.clone(),
            url: from.expanded(url.as_str()),
            headers: resolved(&owner, headers, &start.secrets, from)?
                .into_iter()
                .map(|(name, value)| HttpHeader { name, value })
                .collect(),
        }),
    }))
}

pub(super) fn handed_servers(
    starts: &[McpServerStart],
    mcp: McpCapabilities,
    from: Variables<'_>,
) -> HandedServers {
    let mut handed_over = HandedServers::default();
    for start in starts
        .iter()
        .filter(|start| start.server.state == McpServerState::Enabled)
    {
        match handed(start, mcp, from) {
            Ok(Some(server)) => handed_over.servers.push(server),
            Ok(None) => handed_over.skipped.push(start.server.name.to_string()),
            Err(missing) => handed_over.skipped.push(missing.to_string()),
        }
    }
    handed_over
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::McpServer as DomainServer;

    fn variable(name: &str, value: McpValue) -> McpVariable {
        McpVariable {
            name: name
                .parse()
                .unwrap_or_else(|_| unreachable!("{name} is a variable name")),
            value,
        }
    }

    fn start(name: &str, transport: McpTransport, secret: Option<&str>) -> McpServerStart {
        McpServerStart {
            server: DomainServer {
                name: name
                    .parse()
                    .unwrap_or_else(|_| unreachable!("{name} is a server name")),
                transport,
                state: McpServerState::Enabled,
            },
            secrets: secret
                .and_then(McpSecret::parse)
                .map(|secret| {
                    (
                        "TOKEN"
                            .parse()
                            .unwrap_or_else(|_| unreachable!("a variable name")),
                        secret,
                    )
                })
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn stdio_servers_go_with_their_secrets_and_remote_ones_only_when_the_agent_takes_them() {
        let stdio = start(
            "files",
            McpTransport::Stdio {
                command: "mcp-files".to_owned(),
                args: vec!["--stdio".to_owned()],
                env: vec![variable("TOKEN", McpValue::Secret)],
            },
            Some("s3cret"),
        );
        let remote = start(
            "docs",
            McpTransport::Http {
                url: "https://docs.example.com/mcp"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("a valid url")),
                headers: Vec::new(),
            },
            None,
        );
        let starts = [stdio, remote];
        let local_only = handed_servers(&starts, McpCapabilities::default(), Variables::Here);
        assert!(matches!(
            local_only.servers.as_slice(),
            [McpServer::Stdio(StdioMcpServer { env, .. })] if env[0].value == "s3cret"
        ));
        assert_eq!(local_only.skipped, ["docs"]);
        let both = handed_servers(
            &starts,
            McpCapabilities {
                http: true,
                sse: false,
            },
            Variables::Here,
        );
        assert_eq!(both.servers.len(), 2);
    }

    #[test]
    fn variables_of_a_host_come_from_its_login_environment_not_this_computer() {
        let destination = "dev@box"
            .parse()
            .unwrap_or_else(|_| unreachable!("a valid ssh destination"));
        let link = HostLink::new(
            &krad_domain::Host::Ssh { destination },
            &krad_host::HostContext::default(),
        );
        assert!(matches!(Variables::from(&link), Variables::OnHost(_)));
        assert_eq!(Variables::from(&link).expanded("${HOME}"), "${HOME}");
        assert_ne!(Variables::Here.expanded("${HOME}"), "${HOME}");
    }

    #[test]
    fn a_server_whose_secret_is_gone_is_skipped_with_the_reason() {
        let locked = start(
            "locked",
            McpTransport::Stdio {
                command: "mcp".to_owned(),
                args: Vec::new(),
                env: vec![variable("TOKEN", McpValue::Secret)],
            },
            None,
        );
        let handed_over = handed_servers(&[locked], McpCapabilities::default(), Variables::Here);
        assert_eq!(handed_over.servers, Vec::<McpServer>::new());
        assert!(handed_over.skipped[0].contains("TOKEN"));
    }
}
