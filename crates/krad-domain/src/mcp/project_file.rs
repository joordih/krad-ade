use super::{
    McpServer, McpServerName, McpServerState, McpTransport, McpUrl, McpUrlError, McpValue,
    McpVariable, McpVariableName,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

pub const PROJECT_MCP_FILE: &str = ".mcp.json";

const STDIO_TYPE: &str = "stdio";
const HTTP_TYPE: &str = "http";
const STREAMABLE_HTTP_TYPE: &str = "streamable-http";
const SSE_TYPE: &str = "sse";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McpFileError {
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for McpFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{PROJECT_MCP_FILE} is not valid JSON (line {}, column {})",
            self.line, self.column
        )
    }
}

impl std::error::Error for McpFileError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpSkipReason {
    BadName,
    Malformed,
    NoCommand,
    BadUrl(McpUrlError),
    BadVariableName,
    Sse,
    UnknownType(String),
}

impl fmt::Display for McpSkipReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadName => {
                formatter.write_str("its name has characters other than letters, digits, - _ .")
            }
            Self::Malformed => formatter.write_str("its entry does not have the expected fields"),
            Self::NoCommand => formatter.write_str("it has no command to start"),
            Self::BadUrl(error) => write!(formatter, "its address is not usable: {error}"),
            Self::BadVariableName => {
                formatter.write_str("one of its variable or header names is not valid")
            }
            Self::Sse => formatter.write_str("the legacy SSE transport is not supported"),
            Self::UnknownType(kind) => write!(formatter, "the transport {kind} is not supported"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpSkipped {
    pub name: String,
    pub reason: McpSkipReason,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct McpProjectFile {
    pub servers: Vec<McpServer>,
    pub skipped: Vec<McpSkipped>,
}

#[derive(Deserialize)]
struct RawFile {
    #[serde(default, rename = "mcpServers")]
    servers: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct RawServer {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

enum Kind {
    Stdio,
    Http,
}

fn plain_variables(raw: BTreeMap<String, String>) -> Result<Vec<McpVariable>, McpSkipReason> {
    raw.into_iter()
        .map(|(name, value)| {
            let name = name
                .parse::<McpVariableName>()
                .map_err(|_| McpSkipReason::BadVariableName)?;
            Ok(McpVariable {
                name,
                value: McpValue::Plain(value),
            })
        })
        .collect()
}

fn kind_of(raw: &RawServer) -> Result<Kind, McpSkipReason> {
    match raw.kind.as_deref().map(str::trim) {
        Some(STDIO_TYPE) => Ok(Kind::Stdio),
        Some(HTTP_TYPE | STREAMABLE_HTTP_TYPE) => Ok(Kind::Http),
        Some(SSE_TYPE) => Err(McpSkipReason::Sse),
        Some(other) => Err(McpSkipReason::UnknownType(other.to_owned())),
        None if raw.command.is_none() && raw.url.is_some() => Ok(Kind::Http),
        None => Ok(Kind::Stdio),
    }
}

fn read_server(name: &str, raw: Value) -> Result<McpServer, McpSkipReason> {
    let name = name
        .parse::<McpServerName>()
        .map_err(|_| McpSkipReason::BadName)?;
    let raw: RawServer = serde_json::from_value(raw).map_err(|_| McpSkipReason::Malformed)?;
    let transport = match kind_of(&raw)? {
        Kind::Stdio => {
            let command = raw
                .command
                .map(|command| command.trim().to_owned())
                .filter(|command| !command.is_empty())
                .ok_or(McpSkipReason::NoCommand)?;
            McpTransport::Stdio {
                command,
                args: raw.args,
                env: plain_variables(raw.env)?,
            }
        }
        Kind::Http => McpTransport::Http {
            url: raw
                .url
                .unwrap_or_default()
                .parse::<McpUrl>()
                .map_err(McpSkipReason::BadUrl)?,
            headers: plain_variables(raw.headers)?,
        },
    };
    Ok(McpServer {
        name,
        transport,
        state: McpServerState::Enabled,
    })
}

impl FromStr for McpProjectFile {
    type Err = McpFileError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let raw: RawFile = serde_json::from_str(text).map_err(|error| McpFileError {
            line: error.line(),
            column: error.column(),
        })?;
        let mut file = Self::default();
        for (name, server) in raw.servers {
            match read_server(&name, server) {
                Ok(server) => file.servers.push(server),
                Err(reason) => file.skipped.push(McpSkipped { name, reason }),
            }
        }
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> McpProjectFile {
        text.parse()
            .unwrap_or_else(|error| unreachable!("valid file: {error}"))
    }

    #[test]
    fn stdio_and_http_servers_are_read_with_their_plain_values() {
        let file = read(
            r#"{"mcpServers":{
                "files":{"command":"npx","args":["-y","@scope/server"],"env":{"ROOT":"."}},
                "docs":{"type":"http","url":"https://mcp.example.test/mcp","headers":{"X-Key":"abc"}},
                "local":{"url":"http://127.0.0.1:3000/mcp"}
            }}"#,
        );
        assert!(file.skipped.is_empty(), "{:?}", file.skipped);
        let names: Vec<&str> = file
            .servers
            .iter()
            .map(|server| server.name.as_str())
            .collect();
        assert_eq!(names, ["docs", "files", "local"]);
        let McpTransport::Stdio { command, args, env } = &file.servers[1].transport else {
            unreachable!("files runs a command");
        };
        assert_eq!(command, "npx");
        assert_eq!(args, &["-y", "@scope/server"]);
        assert_eq!(env[0].value, McpValue::Plain(".".to_owned()));
        assert!(matches!(
            file.servers[2].transport,
            McpTransport::Http { .. }
        ));
    }

    #[test]
    fn unsupported_servers_are_skipped_with_a_reason() {
        let file = read(
            r#"{"mcpServers":{
                "old":{"type":"sse","url":"https://mcp.example.test/sse"},
                "odd":{"type":"websocket","url":"wss://x"},
                "empty":{"command":"  "},
                "far":{"type":"http","url":"http://10.0.0.2/mcp"},
                "bad name":{"command":"x"},
                "shape":{"command":["x"]}
            }}"#,
        );
        assert_eq!(file.servers, []);
        let reasons: Vec<(&str, &McpSkipReason)> = file
            .skipped
            .iter()
            .map(|skipped| (skipped.name.as_str(), &skipped.reason))
            .collect();
        assert_eq!(
            reasons,
            [
                ("bad name", &McpSkipReason::BadName),
                ("empty", &McpSkipReason::NoCommand),
                (
                    "far",
                    &McpSkipReason::BadUrl(McpUrlError::PlainHttpToRemote)
                ),
                ("odd", &McpSkipReason::UnknownType("websocket".to_owned())),
                ("old", &McpSkipReason::Sse),
                ("shape", &McpSkipReason::Malformed),
            ]
        );
    }

    #[test]
    fn a_file_without_servers_is_empty_and_broken_json_says_where() {
        assert_eq!(read("{}"), McpProjectFile::default());
        let error = "{\n  \"mcpServers\": ".parse::<McpProjectFile>();
        assert!(error.is_err_and(|error| error.line == 2));
    }
}
