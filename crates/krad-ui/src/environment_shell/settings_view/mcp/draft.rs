use krad_domain::{
    HttpUrlError, McpNameError, McpSecret, McpServer, McpServerName, McpServerState, McpTransport,
    McpUrl, McpValue, McpVariable, McpVariableName,
};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum McpKind {
    #[default]
    Stdio,
    Http,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueKind {
    Plain,
    Secret,
}

pub(super) struct DraftVariable {
    pub(super) name: String,
    pub(super) value: String,
    pub(super) kind: ValueKind,
}

pub(super) struct Draft {
    pub(super) kind: McpKind,
    pub(super) name: String,
    pub(super) command: String,
    pub(super) args: String,
    pub(super) url: String,
    pub(super) variables: Vec<DraftVariable>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DraftError {
    Name(McpNameError),
    NoCommand,
    Url(HttpUrlError),
    VariableName(String),
    EmptySecret(String),
}

impl fmt::Display for DraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(error) => write!(formatter, "Name: {error}"),
            Self::NoCommand => formatter.write_str("Enter the command that starts the server"),
            Self::Url(error) => error.fmt(formatter),
            Self::VariableName(name) => write!(formatter, "{name:?} is not a valid name"),
            Self::EmptySecret(name) => write!(formatter, "Enter a value for {name}"),
        }
    }
}

type Secrets = Vec<(McpVariableName, McpSecret)>;

pub(super) struct ReadyServer {
    pub(super) server: McpServer,
    pub(super) secrets: Secrets,
}

fn variables(
    rows: &[DraftVariable],
    forced: Option<ValueKind>,
) -> Result<(Vec<McpVariable>, Secrets), DraftError> {
    let mut variables = Vec::new();
    let mut secrets = Vec::new();
    for row in rows {
        if row.name.trim().is_empty() && row.value.trim().is_empty() {
            continue;
        }
        let name: McpVariableName = row
            .name
            .parse()
            .map_err(|_| DraftError::VariableName(row.name.trim().to_owned()))?;
        if forced.unwrap_or(row.kind) == ValueKind::Secret {
            let secret = McpSecret::parse(&row.value)
                .ok_or_else(|| DraftError::EmptySecret(name.to_string()))?;
            secrets.push((name.clone(), secret));
            variables.push(McpVariable {
                name,
                value: McpValue::Secret,
            });
        } else {
            variables.push(McpVariable {
                name,
                value: McpValue::Plain(row.value.clone()),
            });
        }
    }
    Ok((variables, secrets))
}

impl Draft {
    pub(super) fn ready(&self) -> Result<ReadyServer, DraftError> {
        let name: McpServerName = self.name.parse().map_err(DraftError::Name)?;
        let (transport, secrets) = match self.kind {
            McpKind::Stdio => {
                let command = self.command.trim();
                if command.is_empty() {
                    return Err(DraftError::NoCommand);
                }
                let (env, secrets) = variables(&self.variables, None)?;
                let transport = McpTransport::Stdio {
                    command: command.to_owned(),
                    args: self.args.split_whitespace().map(str::to_owned).collect(),
                    env,
                };
                (transport, secrets)
            }
            McpKind::Http => {
                let url: McpUrl = self.url.parse().map_err(DraftError::Url)?;
                let (headers, secrets) = variables(&self.variables, Some(ValueKind::Secret))?;
                (McpTransport::Http { url, headers }, secrets)
            }
        };
        Ok(ReadyServer {
            server: McpServer {
                name,
                transport,
                state: McpServerState::Enabled,
            },
            secrets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(kind: McpKind, variables: Vec<DraftVariable>) -> Draft {
        Draft {
            kind,
            name: " files ".to_owned(),
            command: "npx".to_owned(),
            args: " -y  @scope/server ".to_owned(),
            url: "https://mcp.example.test/mcp".to_owned(),
            variables,
        }
    }

    fn row(name: &str, value: &str, kind: ValueKind) -> DraftVariable {
        DraftVariable {
            name: name.to_owned(),
            value: value.to_owned(),
            kind,
        }
    }

    #[test]
    fn a_stdio_server_keeps_plain_values_and_sets_secrets_aside() {
        let ready = draft(
            McpKind::Stdio,
            vec![
                row("ROOT", ".", ValueKind::Plain),
                row("TOKEN", "t-1", ValueKind::Secret),
                row("", "", ValueKind::Plain),
            ],
        )
        .ready()
        .unwrap_or_else(|error| unreachable!("{error}"));
        let McpTransport::Stdio { command, args, env } = &ready.server.transport else {
            unreachable!("a stdio draft");
        };
        assert_eq!(ready.server.name.as_str(), "files");
        assert_eq!(command, "npx");
        assert_eq!(args, &["-y", "@scope/server"]);
        assert_eq!(env.len(), 2);
        assert_eq!(env[1].value, McpValue::Secret);
        assert_eq!(ready.secrets.len(), 1);
        assert_eq!(ready.secrets[0].1.expose(), "t-1");
    }

    #[test]
    fn http_headers_are_always_secret() {
        let ready = draft(
            McpKind::Http,
            vec![row("Authorization", "Bearer x", ValueKind::Plain)],
        )
        .ready()
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            ready.server.transport.variables()[0].value,
            McpValue::Secret
        );
        assert_eq!(ready.secrets.len(), 1);
    }

    #[test]
    fn a_draft_without_its_essentials_is_explained() {
        let mut missing = draft(McpKind::Stdio, Vec::new());
        missing.command = " ".to_owned();
        assert_eq!(missing.ready().err(), Some(DraftError::NoCommand));
        let mut far = draft(McpKind::Http, Vec::new());
        far.url = "http://10.0.0.3/mcp".to_owned();
        assert_eq!(
            far.ready().err(),
            Some(DraftError::Url(HttpUrlError::PlainHttpToRemote))
        );
        let blank = draft(McpKind::Stdio, vec![row("TOKEN", " ", ValueKind::Secret)]);
        assert_eq!(
            blank.ready().err(),
            Some(DraftError::EmptySecret("TOKEN".to_owned()))
        );
    }
}
